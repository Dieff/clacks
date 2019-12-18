use graphql_parser::{parse_query, query, query::Value as GqlValue, schema};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::BTreeMap;

mod execution;
use execution::GqlRunningQuery;
mod introspect;
#[macro_use]
mod resolver_creation;
mod base_types;
pub use base_types::*;

#[derive(Clone, Debug, Default)]
pub struct SchemaTypes {
  pub objects: BTreeMap<String, schema::ObjectType>,
  pub enums: BTreeMap<String, schema::EnumType>,
  pub directives: BTreeMap<String, schema::DirectiveDefinition>,
  pub input_types: BTreeMap<String, schema::InputObjectType>,
}

impl SchemaTypes {
  fn new(doc: schema::Document) -> Self {
    let mut objects = BTreeMap::new();
    let mut enums = BTreeMap::new();
    let mut directives = BTreeMap::new();
    let mut input_types = BTreeMap::new();
    for def in doc.definitions {
      match def {
        schema::Definition::TypeDefinition(t_def) => match t_def {
          schema::TypeDefinition::Object(obj) => {
            objects.insert(obj.name.clone(), obj);
          }
          schema::TypeDefinition::Enum(enu) => {
            enums.insert(enu.name.clone(), enu);
          }
          schema::TypeDefinition::InputObject(input) => {
            input_types.insert(input.name.clone(), input);
          }
          _ => {}
        },
        schema::Definition::DirectiveDefinition(d) => {
          directives.insert(d.name.clone(), d);
        }
        _ => {}
      }
    }

    SchemaTypes {
      objects,
      enums,
      directives,
      input_types,
    }
  }

  fn get_object<'a>(&'a self, on_type: &str) -> Result<&'a schema::ObjectType, GqlQueryErr> {
    self
      .objects
      .get(on_type)
      .ok_or(GqlQueryErr::Type(QueryValidationError::new(
        format!("Could not find type {}", on_type),
        "Type".to_owned(),
      )))
  }
}

#[derive(Default, Clone, Debug)]
pub struct GqlSchema<C> {
  internal_types: SchemaTypes,
  external_types: SchemaTypes,
  resolvers: BTreeMap<String, BTreeMap<String, Resolver<C>>>,
}

impl<C> GqlSchema<C> {
  pub fn new(doc: schema::Document) -> SchemaResult<Self> {
    let external_types = SchemaTypes::new(doc);
    let internal_types = SchemaTypes::new(
      graphql_parser::parse_schema(include_str!("./introspection_defs.graphql")).unwrap(),
    );

    if !external_types.objects.contains_key("Query") {
      return Err(GqlSchemaErr::MissingType("Query".to_owned()));
    }
    let mut schema = GqlSchema {
      internal_types,
      external_types,
      resolvers: BTreeMap::new(),
    };

    let type_rez: BTreeMap<String, Resolver<C>> = type_resolvers!("__Type", {
      description: introspect::r_type_desc,
      ofType: introspect::r_type_ofkind,
      possibleTypes: introspect::r_type_possibletypes,
      enumValues: introspect::r_type_enumvals,
      interfaces: introspect::r_type_interfaces,
      inputFields: introspect::r_type_inputfields,
      fields: introspect::r_type_fields,
    });

    let schema_rez: BTreeMap<String, Resolver<C>> = type_resolvers!("__Schema", {
      queryType: introspect::r_schema_qtype,
      subscriptionType: introspect::r_schema_subtype,
      mutationType: introspect::r_schema_muttype,
      types: introspect::r_schema_types,
      directives: introspect::r_schema_directives,
    });

    let directive_rez: BTreeMap<String, Resolver<C>> =
      type_resolvers!("__Directive", { args: introspect::r_directive_args });

    let inputv_res: BTreeMap<String, Resolver<C>> = type_resolvers!("__InputValue", {
      defaultValue: introspect::r_inputvalue_default,
      type: introspect::r_inputvalue_type,
    });

    let field_rez: BTreeMap<String, Resolver<C>> = type_resolvers!("__Field", {
      args: introspect::r_field_args,
    });

    let query_rez: BTreeMap<String, Resolver<C>> = type_resolvers!("Query", {
      __schema: introspect::r_query_schema,
    });

    schema.resolvers.insert("Query".to_owned(), query_rez);
    schema.resolvers.insert("__Type".to_owned(), type_rez);
    schema.resolvers.insert("__Field".to_owned(), field_rez);
    schema
      .resolvers
      .insert("__InputValue".to_owned(), inputv_res);
    schema.resolvers.insert("__Schema".to_owned(), schema_rez);
    schema
      .resolvers
      .insert("__Directive".to_owned(), directive_rez);
    Ok(schema)
  }

  pub fn add_resolvers(&mut self, resolvers: Vec<Resolver<C>>) -> SchemaResult<()> {
    for resolver in resolvers {
      if !self.external_types.objects.contains_key(&resolver.on_type) {
        return Err(GqlSchemaErr::InvalidResolver);
      } else {
        let obj = self.external_types.objects.get(&resolver.on_type).unwrap();
        let mut found: bool = false;
        for f in &obj.fields {
          if f.name == resolver.field {
            found = true;
          }
        }
        if !found {
          return Err(GqlSchemaErr::InvalidResolver);
        }
      }
      if let Some(inner) = self.resolvers.get_mut(&resolver.on_type) {
        inner.insert(resolver.field.clone(), resolver);
      } else {
        let mut inner = BTreeMap::new();
        let on_type = resolver.on_type.clone();
        inner.insert(resolver.field.clone(), resolver);
        self.resolvers.insert(on_type, inner);
      }
    }
    Ok(())
  }

  fn get_resolvers(&self, on_type: &str, on_field: &str) -> Result<&Resolver<C>, ResolutionErr> {
    Ok(
      self
        .resolvers
        .get(on_type)
        .ok_or(ResolutionErr::new_missing_resolver(on_type, on_field))?
        .get(on_field)
        .ok_or(ResolutionErr::new_missing_resolver(on_type, on_field))?,
    )
  }

  fn get_any_object_type<'a>(
    &'a self,
    on_type: &str,
  ) -> Result<&'a schema::ObjectType, GqlQueryErr> {
    let inner_res = self.internal_types.get_object(on_type);
    if inner_res.is_ok() {
      return inner_res;
    }
    self.external_types.get_object(on_type)
  }

  fn validate_directive(
    &self,
    name: &str,
    args: &Vec<(String, GqlValue)>,
  ) -> Result<(), GqlQueryErr> {
    let directive: &schema::DirectiveDefinition;
    if let Some(d) = self.internal_types.directives.get(name) {
      directive = d
    } else {
      directive = self
        .external_types
        .directives
        .get(name)
        .ok_or(GqlQueryErr::Directive(QueryValidationError::new(
          format!("Invalid directive {}", name),
          "Directive".to_owned(),
        )))?;
    }
    for arg_def in &directive.arguments {
      let arg_val = args
        .iter()
        .find(|(name, _)| name == &arg_def.name)
        .unwrap_or(&("".to_owned(), GqlValue::Null))
        .1
        .clone();
      execution::naive_check_var_type(&arg_def.value_type, &arg_val);
    }
    Ok(())
  }

  fn get_resolution_value_next(
    &self,
    on_type: &str,
    field: &SimpleField,
    context: &mut C,
    data: &BTreeMap<String, query::Value>,
  ) -> ResResult {
    if field.name == "__type" {
      let mut bmap = BTreeMap::new();
      bmap.insert("name".to_owned(), query::Value::String(on_type.to_owned()));
      bmap.insert("kind".to_owned(), GqlValue::Enum("OBJECT".to_owned()));
      return Ok(ResolutionReturn::Type(("__Type".to_owned(), bmap)));
    }
    if field.name == "__typename" {
      return Ok(ResolutionReturn::Scalar(query::Value::String(
        on_type.to_owned(),
      )));
    }
    let resolver = self.get_resolvers(on_type, &field.name)?;
    (resolver.resolve)(data, field.arguments.clone(), context, &self)
  }

  fn resolve_loop_next(
    &self,
    context: &mut C,
    query: &PendingQuery,
    initial_root: Option<GqlRoot>,
  ) -> Result<BTreeMap<String, GqlValue>, ResolutionErr> {
    let mut initial_res = ResolutionContext::new(
      query.on_type.to_owned(),
      "".to_owned(),
      query.fields.clone(),
    );
    if let Some(root) = initial_root {
      initial_res.data = root;
    }
    let mut stack = vec![initial_res];

    'outer: while let Some(mut res_ctx) = stack.pop() {
      while let Some(field) = res_ctx.fields.get(res_ctx.field_res_progress) {
        res_ctx.field_res_progress += 1;
        // TODO validate args
        //self.validate_arguments(res_ctx.cur_type.as_str(), field.name.as_str(), field.arguments);

        // we already have data for that field
        if res_ctx.data.contains_key(&field.name) {
          continue;
        }
        let value =
          self.get_resolution_value_next(&res_ctx.cur_type, &field, context, &res_ctx.data)?;

        match value {
          ResolutionReturn::Scalar(inner_val) => {
            res_ctx.data.insert(field.name.to_owned(), inner_val);
          }
          ResolutionReturn::Type((gql_type, initial_field_results)) => {
            let mut ctx = ResolutionContext::new(
              gql_type.to_owned(),
              field.name.to_owned(),
              field.fields.to_owned(),
            );
            ctx.data = initial_field_results;
            stack.push(res_ctx);
            stack.push(ctx);
            continue 'outer;
          }
          ResolutionReturn::TypeList((gql_type, initial_values)) => {
            // After we push the current resolving type onto the stack,
            // the index of that will be the stack's current length.
            let parent_index = stack.len();
            res_ctx
              .data
              .insert(field.name.clone(), GqlValue::List(vec![]));
            stack.extend(
              initial_values
                .into_iter()
                .map(|t| -> GqlExecResult<ResolutionContext> {
                  let mut rctx = ResolutionContext::new(
                    gql_type.to_owned(),
                    field.name.clone(),
                    field.fields.clone(),
                  );
                  rctx.set_list(parent_index, t);
                  Ok(rctx)
                })
                .collect::<GqlExecResult<Vec<ResolutionContext>>>()?,
            );
            // we insert it here so as to avoid cloning
            // since res_ctx's element are needed in the closure
            stack.insert(parent_index, res_ctx);
            continue 'outer;
          }
        }
      }
      if stack.is_empty() {
        return Ok(res_ctx.data);
      }
      // We have finished resolving a single type, save it and head back into the stack
      //trim_selection_fields(&res_ctx.fields, &mut res_ctx.data);
      if let Some(parent_index) = res_ctx.in_list {
        let parent_data = &mut stack[parent_index].data;
        match parent_data.get_mut(&res_ctx.map_key) {
          Some(GqlValue::List(l)) => {
            l.push(GqlValue::Object(res_ctx.data));
          }
          _ => {
            panic!("Found a list that was not a list!");
          }
        }
      } else {
        let last_index = stack.len() - 1;
        stack[last_index]
          .data
          .insert(res_ctx.map_key, query::Value::Object(res_ctx.data));
      }
    }
    Ok(BTreeMap::new())
  }

  fn process_field(
    &self,
    field: &query::Field,
    on_type: &str,
    exec: &GqlRunningQuery,
  ) -> Result<Vec<SimpleField>, GqlQueryErr> {
    let fields = exec.fields_from_selectionset(&field.selection_set, on_type)?;
    let full_type = self.get_any_object_type(on_type)?;
    let field_type: query::Type;
    if field.name == "__typename" {
      field_type = query::Type::NamedType("String".to_owned());
    } else if field.name == "__type" {
      field_type = query::Type::NamedType("__Type".to_owned());
    } else if field.name == "__schema" {
      field_type = query::Type::NamedType("__Schema".to_owned());
    } else {
      field_type = full_type
        .fields
        .iter()
        .find(|f| f.name == field.name)
        .ok_or(GqlQueryErr::Field(QueryValidationError::new(
          format!("Could not find field {} on type {}", field.name, on_type),
          "Field".to_owned(),
        )))?
        .field_type
        .clone();
    }
    let mut cur_type = field_type;
    let final_type = loop {
      match cur_type {
        query::Type::NamedType(name) => {
          break name;
        }
        query::Type::ListType(l) => {
          cur_type = *l;
        }
        query::Type::NonNullType(l) => {
          cur_type = *l;
        }
      }
    };
    fields
      .into_iter()
      .map(|f| {
        for d in &f.directives {
          self.validate_directive(&d.name, &d.arguments)?;
        }
        Ok(SimpleField {
          name: f.name.clone(),
          directives: f.directives.clone(),
          arguments: f.arguments.clone().into_iter().fold(
            BTreeMap::new(),
            |mut map, (name, val)| {
              map.insert(name, val);
              map
            },
          ),
          fields: self.process_field(&f, &final_type, exec)?,
        })
      })
      .collect::<Result<Vec<SimpleField>, GqlQueryErr>>()
  }

  pub fn resolve(
    &self,
    context: &mut C,
    req: GqlRequest,
    root: Option<GqlRoot>,
  ) -> Result<JsonValue, ResolutionErr> {
    let query_ast =
      parse_query(&req.query).map_err(|e| ResolutionErr::QueryParseIssue(format!("{:?}", e)))?;
    let mut query_info = GqlRunningQuery::new(query_ast);
    query_info
      .parse_fragments()
      .map_err(|a| ResolutionErr::QueryValidation(a))?;
    query_info
      .parse_variables(req.variables)
      .map_err(|a| ResolutionErr::QueryValidation(a))?;

    // Contains any Queries, Mutations, or Subscriptions in the request
    let queries = query_info.get_initial_items()?;

    let mut data: JsonMap<String, JsonValue> = JsonMap::new();
    for queree in queries {
      let pending_query = PendingQuery {
        on_type: &query_info.starting_type,
        fields: queree
          .initial_fields
          .clone()
          .into_iter()
          .map(|f| {
            Ok(SimpleField {
              name: f.name.clone(),
              arguments: f
                .arguments
                .iter()
                .fold(BTreeMap::new(), |mut map, (name, val)| {
                  map.insert(name.to_owned(), val.to_owned());
                  map
                }),
              directives: f.directives.clone(),
              fields: self.process_field(&f, &query_info.starting_type, &query_info)?,
            })
          })
          .collect::<Result<Vec<SimpleField>, GqlQueryErr>>()?,
      };

      let mut res = self.resolve_loop_next(context, &pending_query, root.clone())?;
      for field in &pending_query.fields {
        let val = res.get_mut(&field.name).unwrap();
        // And extra fields that weren't requested are removed here
        sparsify_return(val, &field);
        // convert from GqlValue to JsonValue
        let jdata = execution::gql_to_json(val.to_owned())
          .map_err(|_| ResolutionErr::QueryResult(format!("Could not encode result to JSON")))?;
        data.insert(field.name.to_owned(), jdata);
      }
    }

    Ok(JsonValue::Object(data))
  }
}

fn sparsify_return(val: &mut GqlValue, field: &SimpleField) {
  if let GqlValue::Object(obj) = val {
    let mut extra_keys = Vec::new();
    for (key, mut val) in obj.iter_mut() {
      match field.fields.iter().find(|f| f.name == *key) {
        Some(field) => {
          sparsify_return(&mut val, &field);
        }
        None => {
          extra_keys.push(key.clone());
        }
      }
    }
    for key in extra_keys {
      obj.remove(&key);
    }
  }
}

#[derive(Clone, Debug)]
struct SimpleField {
  name: String,
  directives: Vec<query::Directive>,
  arguments: BTreeMap<String, GqlValue>,
  fields: Vec<SimpleField>,
}

#[derive(Clone, Debug)]
struct PendingQuery<'a> {
  on_type: &'a str,
  fields: Vec<SimpleField>,
}

#[derive(Default)]
struct ResolutionContext {
  cur_type: String,
  map_key: String,
  fields: Vec<SimpleField>,
  field_res_progress: usize,
  data: BTreeMap<String, query::Value>,
  in_list: Option<usize>,
}

impl ResolutionContext {
  fn new(cur_type: String, map_key: String, fields: Vec<SimpleField>) -> Self {
    ResolutionContext {
      cur_type,
      map_key,
      fields,
      ..Default::default()
    }
  }

  fn set_list(&mut self, index: usize, data: BTreeMap<String, GqlValue>) {
    self.in_list = Some(index);
    self.data = data;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::{from_str, json, to_string};

  #[test]
  fn simple_query() {
    let schema = include_str!("../../tests/simple_schema.graphql");
    let query_str = include_str!("../../tests/simple_query.graphql");
    let q_msg = GqlRequest {
      query: query_str.to_owned(),
      operation_name: None,
      variables: None,
    };
    let doc = graphql_parser::parse_schema(schema).unwrap();

    type Context = String;
    let mut p_schema: GqlSchema<Context> = GqlSchema::new(doc).unwrap();
    assert_eq!(p_schema.external_types.objects.len(), 1);
    assert_eq!(p_schema.external_types.enums.len(), 0);

    fn resolve_query_message(
      _root: &GqlRoot,
      _args: GqlArgs,
      ctx: &mut Context,
      _r: &GqlSchema<Context>,
    ) -> ResResult {
      return Ok(ResolutionReturn::Scalar(query::Value::String(
        ctx.to_owned(),
      )));
    }

    let r = Resolver::new(Box::new(resolve_query_message), "Query", "message");

    assert!(p_schema.add_resolvers(vec![r]).is_ok());
    let result = p_schema
      .resolve(&mut "Hello world!".to_owned(), q_msg, None)
      .unwrap();
    let expected = r#"
      {"message": "Hello world!"} 
    "#;
    assert_eq!(result, from_str::<JsonValue>(expected).unwrap());
  }

  #[test]
  fn introspection() {
    let schema = include_str!("../../tests/med_schema.graphql");
    let query_str = include_str!("../../tests/introspection_query.graphql");
    let q_msg = GqlRequest {
      query: query_str.to_owned(),
      operation_name: None,
      variables: None,
    };
    let doc = graphql_parser::parse_schema(schema).unwrap();

    // just a random type for testing
    type Context = Vec<i8>;
    let p_schema: GqlSchema<Context> = GqlSchema::new(doc).unwrap();

    let schema_data = p_schema.resolve(&mut Vec::new(), q_msg, None).unwrap();
    if let JsonValue::Object(data) = schema_data {
      assert!(to_string(&JsonValue::Object(data)).is_ok());
    } else {
      panic!("Incorrect object return from resolver");
    }
  }

  #[test]
  fn subscription() {
    let mut schema: GqlSchema<i32> = GqlSchema::new(
      graphql_parser::parse_schema(include_str!("../../tests/subscription_schema.graphql"))
        .unwrap(),
    )
    .unwrap();

    fn resolve_query_message(
      root: &GqlRoot,
      _args: GqlArgs,
      ctx: &mut i32,
      _r: &GqlSchema<i32>,
    ) -> ResResult {
      let mut bmap = BTreeMap::new();
      bmap.insert(
        "content".to_owned(),
        root.get("content").unwrap().to_owned(),
      );
      bmap.insert("id".to_owned(), GqlValue::String(format!("{}", ctx)));
      bmap.insert("jam".to_owned(), GqlValue::Boolean(false));
      Ok(ResolutionReturn::Type(("Message".to_owned(), bmap)))
    }

    schema
      .add_resolvers(vec![Resolver::new(
        Box::new(resolve_query_message),
        "Subscription",
        "newMessage",
      )])
      .unwrap();
    let query = r#"
      subscription {
        newMessage {
          id
          content
        }
      }
    "#;
    let req = GqlRequest {
      variables: None,
      query: query.to_owned(),
      operation_name: None,
    };
    let mut initial_root = BTreeMap::new();
    initial_root.insert(
      "content".to_owned(),
      GqlValue::String("Hello world!".to_owned()),
    );
    if let JsonValue::Object(obj) = schema.resolve(&mut 10, req, Some(initial_root)).unwrap() {
      if let JsonValue::Object(new_msg) = obj.get("newMessage").unwrap() {
        assert_eq!(new_msg.get("content"), Some(&json!("Hello world!")));
        assert!(!new_msg.contains_key("jam"));
        return;
      }
    }
    panic!("resolve did not return an object");
  }
}
