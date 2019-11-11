use super::*;

const BUILTIN_SCALARS: &'static [&str] = &["String", "Boolean", "ID", "Int", "Float"];

pub fn r_type_desc<C>(
  root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  if let Some(query::Value::String(parent)) = root.get("name") {
    if BUILTIN_SCALARS.contains(&parent.as_str()) {
      return Ok(ResolutionReturn::Scalar(GqlValue::String(
        "Scalar type".to_owned(),
      )));
    }
    if let Some(enum_def) = schema.enums.get(parent) {
      return match &enum_def.description {
        Some(desc) => Ok(ResolutionReturn::Scalar(query::Value::String(desc.clone()))),
        None => Ok(ResolutionReturn::Scalar(query::Value::Null)),
      };
    }
    if let Some(object_def) = schema.objects.get(parent) {
      return match &object_def.description {
        Some(desc) => Ok(ResolutionReturn::Scalar(query::Value::String(desc.clone()))),
        None => Ok(ResolutionReturn::Scalar(query::Value::Null)),
      };
    }
    if let Some(input_def) = schema.input_types.get(parent) {
      return match &input_def.description {
        Some(desc) => Ok(ResolutionReturn::Scalar(query::Value::String(desc.clone()))),
        None => Ok(ResolutionReturn::Scalar(query::Value::Null)),
      };
    }
    return Err(ResolutionErr::new_missing_type(parent));
  }
  Ok(ResolutionReturn::Scalar(query::Value::Null))
}

pub fn r_type_ofkind<C>(
  root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  // TODO: do we always need a name?
  if let (Some(query::Value::Enum(type_kind)), Some(query::Value::String(name))) =
    (root.get("kind"), root.get("name"))
  {
    match type_kind.as_str() {
      "LIST" | "NON_NULL" => {
        let mut bmap = BTreeMap::new();
        bmap.insert("name".to_owned(), GqlValue::String(name.clone()));
        if BUILTIN_SCALARS.contains(&name.as_str()) {
          bmap.insert("kind".to_owned(), GqlValue::Enum("SCALAR".to_owned()));
        } else if schema.enums.contains_key(name) {
          bmap.insert("kind".to_owned(), GqlValue::Enum("ENUM".to_owned()));
        } else if schema.objects.contains_key(name) {
          bmap.insert("kind".to_owned(), GqlValue::Enum("OBJECT".to_owned()));
        }

        return Ok(ResolutionReturn::Type(("__Type".to_owned(), bmap)));
      }
      _ => {
        return Ok(ResolutionReturn::Scalar(GqlValue::Null));
      }
    }
  }
  Err(ResolutionErr::new_invalid_field("__Type", "kind | name"))
}

pub fn r_type_possibletypes<C>(
  root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  Ok(ResolutionReturn::Scalar(GqlValue::Null))
}

pub fn r_type_enumvals<C>(
  root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  // TODO: do we always need a name?
  if let (Some(query::Value::Enum(type_kind)), Some(query::Value::String(name))) =
    (root.get("kind"), root.get("name"))
  {
    match type_kind.as_str() {
      "ENUM" => {
        // the definition of the enum as parsed from the AST
        let en = schema
          .enums
          .get(name)
          .ok_or(ResolutionErr::new_missing_type(name.as_str()))?;

        let mut res = Vec::new();
        for value in &en.values {
          let mut bmap = BTreeMap::new();
          bmap.insert("name".to_owned(), GqlValue::String(value.name.to_owned()));
          if let Some(desc) = &value.description {
            bmap.insert("description".to_owned(), GqlValue::String(desc.to_owned()));
          } else {
            bmap.insert("description".to_owned(), GqlValue::Null);
          }
          bmap.insert("isDeprecated".to_owned(), GqlValue::Boolean(false));
          bmap.insert("deprecationReason".to_owned(), GqlValue::Null);
          res.push(bmap);
        }

        return Ok(ResolutionReturn::TypeList(("__EnumValue".to_owned(), res)));
      }
      _ => {
        return Ok(ResolutionReturn::Scalar(GqlValue::Null));
      }
    }
  }
  Err(ResolutionErr::new_invalid_field("__Type", "name | kind"))
}

pub fn r_type_interfaces<C>(
  root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  // TODO: do we always need a name?
  match (root.get("kind"), root.get("name")) {
    (Some(GqlValue::Enum(ref k)), Some(GqlValue::String(name))) if k == "OBJECT" => {
      Ok(ResolutionReturn::TypeList(("__Type".to_owned(), vec![])))
    }
    (Some(GqlValue::Enum(_)), Some(_)) => Ok(ResolutionReturn::Scalar(GqlValue::Null)),
    (_, _) => Err(ResolutionErr::new_invalid_field("__Type", "name | kind")),
  }
}

pub fn r_type_inputfields<C>(
  root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  if let (Some(GqlValue::Enum(kind)), Some(GqlValue::String(name))) =
    (root.get("kind"), root.get("name"))
  {
    if kind == "INPUT_OBJECT" {
      let input_obj_def = schema.input_types.get(name).unwrap();
      let mut res = Vec::new();
      for field in &input_obj_def.fields {
        let mut bmap = BTreeMap::new();
        bmap.insert("name".to_owned(), GqlValue::String(field.name.clone()));
        bmap.insert(
          "description".to_owned(),
          field
            .description
            .as_ref()
            .map(|d| GqlValue::String(d.to_owned()))
            .unwrap_or(GqlValue::Null),
        );
        bmap.insert(
          "defaultValue".to_owned(),
          field
            .default_value
            .as_ref()
            .map(|v| GqlValue::String(value_to_string(v)))
            .unwrap_or(GqlValue::Null),
        );
        bmap.insert("parentTypename".to_owned(), GqlValue::String(name.clone()));
        res.push(bmap);
      }
      return Ok(ResolutionReturn::TypeList(("__InputValue".to_owned(), res)));
    }
    return Ok(ResolutionReturn::Scalar(GqlValue::Null));
  }
  Err(ResolutionErr::new_invalid_field("__Type", "kind"))
}

fn convert_field_type<C>(schema: &GqlSchema<C>, field_type: query::Type) -> GqlObj {
  let mut result = BTreeMap::new();
  result.insert(
    "name".to_owned(),
    GqlValue::String("generic_type".to_owned()),
  );
  match field_type {
    query::Type::ListType(inner_type) => {
      let of_kind = convert_field_type(schema, *inner_type);
      result.insert("ofType".to_owned(), GqlValue::Object(of_kind));
      result.insert("kind".to_owned(), GqlValue::Enum("LIST".to_owned()));
    }
    query::Type::NonNullType(inner_type) => {
      let of_kind = convert_field_type(schema, *inner_type);
      result.insert("ofType".to_owned(), GqlValue::Object(of_kind));
      result.insert("kind".to_owned(), GqlValue::Enum("NON_NULL".to_owned()));
    }
    query::Type::NamedType(type_name) => {
      result.insert("name".to_owned(), GqlValue::String(type_name.clone()));
      if BUILTIN_SCALARS.contains(&type_name.as_str()) {
        result.insert("kind".to_owned(), GqlValue::Enum("SCALAR".to_owned()));
      }
      if schema.objects.contains_key(&type_name) {
        result.insert("kind".to_owned(), GqlValue::Enum("OBJECT".to_owned()));
      } else if schema.enums.contains_key(&type_name) {
        result.insert("kind".to_owned(), GqlValue::Enum("ENUM".to_owned()));
      }
    }
  }
  result
}

fn value_to_string(val: &GqlValue) -> String {
  match val {
    GqlValue::Variable(n) => n.clone(),
    GqlValue::Boolean(b) => format!("{}", b),
    GqlValue::Float(f) => format!("{}", f),
    GqlValue::Int(i) => format!("{:?}", i),
    GqlValue::Null => "null".to_owned(),
    GqlValue::String(s) => s.clone(),
    _ => "".to_owned(),
  }
}

pub fn r_type_fields<C>(
  root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  match (root.get("kind"), root.get("name")) {
    (Some(GqlValue::Enum(ref k)), Some(GqlValue::String(name))) if k == "OBJECT" => {
      if let Some(def) = schema.objects.get(name.as_str()) {
        return Ok(ResolutionReturn::TypeList((
          "__Field".to_owned(),
          def
            .fields
            .iter()
            .map(|field| {
              let tmap = convert_field_type(schema, field.field_type.clone());
              let mut bmap = BTreeMap::new();
              bmap.insert("name".to_owned(), GqlValue::String(field.name.clone()));
              bmap.insert("parentTypename".to_owned(), GqlValue::String(name.clone()));
              if let Some(desc) = &field.description {
                bmap.insert("description".to_owned(), GqlValue::String(desc.clone()));
              } else {
                bmap.insert("description".to_owned(), GqlValue::Null);
              }
              bmap.insert("type".to_owned(), GqlValue::Object(tmap));
              bmap.insert("isDeprecated".to_owned(), GqlValue::Boolean(false));
              bmap.insert("deprecationReason".to_owned(), GqlValue::Null);
              bmap
            })
            .collect(),
        )));
      }
      return Err(ResolutionErr::new_missing_type(&name));
    }
    _ => Ok(ResolutionReturn::Scalar(GqlValue::Null)),
  }
}

pub fn r_field_args<C>(
  root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  if let (Some(GqlValue::String(field_name)), Some(GqlValue::String(type_name))) =
    (root.get("name"), root.get("parentTypename"))
  {
    let obj_def = schema
      .objects
      .get(type_name)
      .ok_or(ResolutionErr::new_missing_type(type_name))?;

    let field = obj_def
      .fields
      .iter()
      .find(|f| f.name == *field_name)
      .ok_or(ResolutionErr::new_invalid_field(type_name, field_name))?;

    return Ok(ResolutionReturn::TypeList((
      "__InputValue".to_owned(),
      field
        .arguments
        .iter()
        .map(|arg| {
          let mut tmap = BTreeMap::new();
          tmap.insert("name".to_owned(), GqlValue::String(arg.name.to_owned()));
          if let Some(desc) = &arg.description {
            tmap.insert("description".to_owned(), GqlValue::String(desc.clone()));
          } else {
            tmap.insert("description".to_owned(), GqlValue::Null);
          }
          if let Some(default_val) = &arg.default_value {
            tmap.insert(
              "defaultValue".to_owned(),
              GqlValue::String(value_to_string(default_val)),
            );
          } else {
            tmap.insert("defaultValue".to_owned(), GqlValue::Null);
          }
          tmap.insert(
            "type".to_owned(),
            full_input_type_resolver(&arg.value_type, schema),
          );
          tmap
        })
        .collect(),
    )));
  }
  Err(ResolutionErr::new_invalid_field("?", "?"))
}

fn schema_type(target_type: &str, schema: &BTreeMap<String, schema::ObjectType>) -> ResResult {
  if schema.contains_key(target_type) {
    let mut bmap = BTreeMap::new();
    bmap.insert(
      "name".to_owned(),
      query::Value::String(target_type.to_owned()),
    );
    bmap.insert("kind".to_owned(), GqlValue::Enum("OBJECT".to_owned()));
    return Ok(ResolutionReturn::Type(("__Type".to_owned(), bmap)));
  }
  Ok(ResolutionReturn::Scalar(query::Value::Null))
}

pub fn r_schema_qtype<C>(
  _root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  schema_type("Query", &schema.objects)
}

pub fn r_schema_subtype<C>(
  _root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  schema_type("Subscription", &schema.objects)
}

pub fn r_schema_muttype<C>(
  _root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  schema_type("Mutation", &schema.objects)
}

pub fn r_schema_directives<C>(
  _root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  _schema: &GqlSchema<C>,
) -> ResResult {
  Ok(ResolutionReturn::TypeList((
    "__Directive".to_owned(),
    vec![],
  )))
}

pub fn r_schema_types<C>(
  _root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  let mut res_items = Vec::new();
  schema.objects.keys().for_each(|type_name| {
    let mut bmap = BTreeMap::new();
    bmap.insert(
      "name".to_owned(),
      query::Value::String(type_name.to_owned()),
    );
    bmap.insert("kind".to_owned(), GqlValue::Enum("OBJECT".to_owned()));
    res_items.push(bmap);
  });
  schema.input_types.keys().for_each(|type_name| {
    let mut bmap = BTreeMap::new();
    bmap.insert(
      "name".to_owned(),
      query::Value::String(type_name.to_owned()),
    );
    bmap.insert(
      "kind".to_owned(),
      query::Value::Enum("INPUT_OBJECT".to_owned()),
    );
    res_items.push(bmap);
  });
  Ok(ResolutionReturn::TypeList(("__Type".to_owned(), res_items)))
}

pub fn r_query_schema<C>(
  _root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  _schema: &GqlSchema<C>,
) -> ResResult {
  Ok(ResolutionReturn::Type((
    "__Schema".to_owned(),
    BTreeMap::new(),
  )))
}

pub fn r_directive_args<C>(
  root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  match root.get("name") {
    Some(GqlValue::String(name)) => {
      let def = schema
        .directives
        .get(name)
        .ok_or(ResolutionErr::new_invalid_field("__Directive", name))?;
      let l = def
        .arguments
        .clone()
        .into_iter()
        .map(|arg| {
          let mut bmap = BTreeMap::new();
          bmap.insert("name".to_owned(), GqlValue::String(arg.name));
          bmap
        })
        .collect();
      Ok(ResolutionReturn::TypeList(("__InputValue".to_owned(), l)))
    }
    _ => Err(ResolutionErr::new_invalid_field("__Directive", "name")),
  }
}

pub fn r_inputvalue_default<C>(
  root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  _schema: &GqlSchema<C>,
) -> ResResult {
  match root.get("name") {
    Some(GqlValue::String(name)) => Ok(ResolutionReturn::Scalar(GqlValue::Null)),
    _ => Err(ResolutionErr::new_invalid_field("__InputValue", "name")),
  }
}

pub fn r_inputvalue_type<C>(
  root: &BTreeMap<String, query::Value>,
  _args: Vec<(String, query::Value)>,
  _ctx: &mut C,
  schema: &GqlSchema<C>,
) -> ResResult {
  if let (Some(GqlValue::String(my_name)), Some(GqlValue::String(parent_name))) =
    (root.get("name"), root.get("parentTypename"))
  {
    if let Some(input_def) = schema.input_types.get(parent_name) {
      let field = input_def
        .fields
        .iter()
        .find(|x| x.name == *my_name)
        .unwrap();
      return Ok(ResolutionReturn::Scalar(full_input_type_resolver(
        &field.value_type,
        schema,
      )));
    }
  }
  Err(ResolutionErr::new_invalid_field("__InputValue", "name"))
}

fn full_input_type_resolver<C>(value_type: &query::Type, schema: &GqlSchema<C>) -> GqlValue {
  let mut bmap = BTreeMap::new();
  bmap.insert("name".to_owned(), GqlValue::String(format!("")));
  match &value_type {
    query::Type::NamedType(name) => {
      bmap.insert("name".to_owned(), GqlValue::String(name.clone()));
      if BUILTIN_SCALARS.contains(&name.as_str()) {
        bmap.insert("kind".to_owned(), GqlValue::Enum("SCALAR".to_owned()));
      } else {
        // TODO: this should be reliable
        let input_type = schema.input_types.get(name).unwrap();
        bmap.insert("kind".to_owned(), GqlValue::Enum("INPUT_OBJECT".to_owned()));
        bmap.insert("ofType".to_owned(), GqlValue::Null);
      }
    }
    query::Type::NonNullType(nnt) => {
      bmap.insert("kind".to_owned(), GqlValue::Enum("NON_NULL".to_owned()));
      bmap.insert("ofType".to_owned(), full_input_type_resolver(nnt, schema));
    }
    query::Type::ListType(lt) => {
      bmap.insert("kind".to_owned(), GqlValue::Enum("LIST".to_owned()));
      bmap.insert("ofType".to_owned(), full_input_type_resolver(lt, schema));
    }
  }
  GqlValue::Object(bmap)
}
