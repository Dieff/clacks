use graphql_parser::{parse_query, query, query::Value as GqlValue, schema};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::BTreeMap;
use std::fmt;

mod execution;
use execution::GqlRunningQuery;
mod introspect;
#[macro_use]
mod resolver_creation;
mod base_types;
pub use base_types::*;

#[derive(Default, Clone)]
pub struct GqlSchema<C> {
  objects: BTreeMap<String, schema::ObjectType>,
  enums: BTreeMap<String, schema::EnumType>,
  directives: BTreeMap<String, schema::DirectiveDefinition>,
  input_types: BTreeMap<String, schema::InputObjectType>,
  resolvers: BTreeMap<String, BTreeMap<String, Resolver<C>>>,
}

impl<C: 'static> GqlSchema<C> {
  pub fn new(doc: schema::Document) -> SchemaResult<Self> {
    let mut objects = BTreeMap::new();
    let mut enums = BTreeMap::new();
    let mut directives = BTreeMap::new();
    let mut input_types = BTreeMap::new();

    let defs = doc.definitions.clone();

    for def in defs {
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

    if !objects.contains_key("Query") {
      return Err(GqlSchemaErr::MissingType("Query".to_owned()));
    }
    let mut schema = GqlSchema {
      objects,
      enums,
      directives,
      input_types,
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
      if !self.objects.contains_key(&resolver.on_type) {
        return Err(GqlSchemaErr::InvalidResolver);
      } else {
        let obj = self.objects.get(&resolver.on_type).unwrap();
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

  // TODO: increase ergonomics
  /*
  fn get_field_definition(
    &self,
    type_name: &str,
    field_name: &str,
  ) -> Result<schema::Field, ResolutionErr> {
    let gql_type = self
      .objects
      .get(type_name)
      .ok_or(ResolutionErr::QueryValidation(GqlQueryErr::Field(
        QueryValidationError::new(
          format!("could not find type {}", type_name),
          type_name.to_owned(),
          None,
        ),
      )))?;

    let mut fields: HashMap<String, schema::Field> = HashMap::new();
    for field in &gql_type.fields {
      fields.insert(field.name.to_owned(), field.to_owned());
    }
    Ok(
      fields
        .get(type_name)
        .ok_or(ResolutionErr::QueryValidation(GqlQueryErr::Field(
          QueryValidationError::new(
            format!("could not find field {} on type {}", field_name, type_name),
            type_name.to_owned(),
            None,
          ),
        )))?
        .to_owned(),
    )
  }

  fn validate_arg_type(&self) {
    //
  }

  fn validate_arguments(
    &self,
    gql_type: &str,
    field_name: &str,
    arguments: &Vec<GqlValue>,
  ) -> bool {
    if let Ok(field_def) = self.get_field_definition(gql_type, field_name) {
      //
    }
    false
  }*/

  fn get_resolution_value(
    &self,
    on_type: &str,
    field: &query::Field,
    context: &mut C,
    data: &BTreeMap<String, query::Value>,
  ) -> ResResult {
    if field.name == "__type" {
      let mut bmap = BTreeMap::new();
      bmap.insert("name".to_owned(), query::Value::String(on_type.to_owned()));
      bmap.insert("kind".to_owned(), GqlValue::Enum("OBJECT".to_owned()));
      return Ok(ResolutionReturn::Type(("__Type".to_owned(), bmap)));
    }
    //println!("{}.{}", on_type, field.name);
    if field.name == "__typename" {
      return Ok(ResolutionReturn::Scalar(query::Value::String(
        on_type.to_owned(),
      )));
    }
    let resolver = self.get_resolvers(on_type, &field.name)?;
    (resolver.resolve)(data, field.arguments.clone(), context, &self)
  }

  fn resolve_loop(
    &self,
    context: &mut C,
    initial_fields: Vec<query::Field>,
    query_info: &GqlRunningQuery,
    initial_type: &str,
    initial_root: Option<GqlRoot>,
  ) -> Result<BTreeMap<String, GqlValue>, ResolutionErr> {
    let mut initial_res =
      ResolutionContext::new(initial_type.to_owned(), "".to_owned(), initial_fields);
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
        let value = self.get_resolution_value(&res_ctx.cur_type, &field, context, &res_ctx.data)?;

        match value {
          ResolutionReturn::Scalar(inner_val) => {
            res_ctx.data.insert(field.name.to_owned(), inner_val);
          }
          ResolutionReturn::Type((gql_type, initial_field_results)) => {
            let mut ctx = ResolutionContext::new(
              gql_type.to_owned(),
              field.name.to_owned(),
              query_info.fields_from_selectionset(&field.selection_set, &gql_type)?,
            );
            ctx.data = initial_field_results;
            stack.push(res_ctx.clone());
            stack.push(ctx);
            continue 'outer;
          }
          ResolutionReturn::List(l) => {
            res_ctx
              .data
              .insert(field.name.to_owned(), GqlValue::List(l));
          }
          ResolutionReturn::TypeList((gql_type, initial_values)) => {
            // After we push the current resolving type onto the stack,
            // the index of that will be the stack's current length.
            let parent_index = stack.len();
            res_ctx
              .data
              .insert(field.name.clone(), GqlValue::List(vec![]));
            stack.push(res_ctx.clone());
            stack.extend(
              initial_values
                .into_iter()
                .map(|t| -> GqlExecResult<ResolutionContext> {
                  let fields =
                    query_info.fields_from_selectionset(&field.selection_set, &gql_type)?;
                  let mut rctx =
                    ResolutionContext::new(gql_type.to_owned(), field.name.clone(), fields);
                  rctx.set_list(parent_index, t);
                  Ok(rctx)
                })
                .collect::<GqlExecResult<Vec<ResolutionContext>>>()?,
            );
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

  pub fn resolve(
    &self,
    mut context: C,
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
      // key for the query in the final data map
      for (key, val) in self
        .resolve_loop(
          &mut context,
          queree.initial_fields,
          &query_info,
          &query_info.starting_type,
          root.clone(),
        )?
        .into_iter()
      {
        let jdata = execution::gql_to_json(val)
          .map_err(|_| ResolutionErr::QueryResult(format!("Could not encode result to JSON")))?;
        data.insert(key, jdata);
      }
    }

    Ok(JsonValue::Object(data))
  }
}

impl<C> fmt::Debug for GqlSchema<C> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "GqlSchema {{ objects: {:?}, enums: {:?}, input_types: {:?} }}",
      self.objects, self.enums, self.input_types
    )
  }
}

#[derive(Clone, Debug)]
struct ResolutionContext {
  cur_type: String,
  map_key: String,
  fields: Vec<query::Field>,
  field_res_progress: usize,
  data: BTreeMap<String, query::Value>,
  in_list: Option<usize>,
}

impl ResolutionContext {
  fn new(cur_type: String, map_key: String, fields: Vec<query::Field>) -> Self {
    ResolutionContext {
      cur_type,
      map_key,
      fields,
      field_res_progress: 0,
      data: BTreeMap::new(),
      in_list: None,
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
  use serde_json::{from_str, to_string};

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
    assert!(p_schema.objects.len() == 1);
    assert!(p_schema.enums.len() == 0);

    fn resolve_query_message(
      _root: &BTreeMap<String, query::Value>,
      _args: Vec<(String, query::Value)>,
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
      .resolve("Hello world!".to_owned(), q_msg, None)
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

    let schema_data = p_schema.resolve(Vec::new(), q_msg, None).unwrap();
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
    let res = schema.resolve(10, req, Some(initial_root)).unwrap();
    if let JsonValue::Object(obj) = res {
      assert!(obj.contains_key("newMessage"));
    } else {
      panic!("resolve did not return an object");
    }
  }
}
