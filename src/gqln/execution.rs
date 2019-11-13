#![allow(non_snake_case)]

use graphql_parser::query;
use serde_json::{json, Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::collections::{BTreeMap, HashMap};

use crate::gqln::base_types::*;

// TODO: make this function smart
pub fn naive_check_var_type(var_type: &query::Type, var_value: &GqlValue) -> bool {
  match (var_type, var_value) {
    (_, GqlValue::Variable(_)) => false,
    (query::Type::NamedType(_), GqlValue::Null) => true,
    (query::Type::ListType(_), GqlValue::Null) => true,
    (query::Type::NamedType(l), GqlValue::String(_)) if l == "String" => true,
    (query::Type::NamedType(l), GqlValue::Float(_)) if l == "Float" => true,
    (query::Type::NamedType(l), GqlValue::Int(_)) if l == "Integer" => true,
    (query::Type::NamedType(l), GqlValue::Boolean(_)) if l == "Boolean" => true,
    // naive
    (query::Type::NamedType(_), GqlValue::Enum(_)) => true,
    // naive
    (query::Type::NamedType(_), GqlValue::Object(_)) => true,
    (query::Type::NonNullType(j), v) => !(v == &GqlValue::Null) && naive_check_var_type(j, v),
    (query::Type::ListType(j), GqlValue::List(v)) => {
      if v.len() == 0 {
        return true;
      }
      let test_case = &v[0];
      return naive_check_var_type(j, test_case);
    }
    (query::Type::ListType(_), _) => false,
    _ => false,
  }
}

pub struct FieldSelection {
  pub name: Option<String>,
  pub initial_fields: Vec<query::Field>,
}

impl FieldSelection {
  fn new(name: Option<String>, initial_fields: Vec<query::Field>) -> Self {
    Self {
      name,
      initial_fields,
    }
  }
}

/// Represents a single graphql query
#[derive(Debug, Clone)]
pub struct GqlRunningQuery {
  variables: HashMap<String, GqlValue>,
  fragments: HashMap<String, query::FragmentDefinition>,
  fragment_fields: HashMap<String, Vec<query::Field>>,
  query_ast: query::Document,
  pub starting_type: String,
}

impl GqlRunningQuery {
  pub fn new(doc: query::Document) -> Self {
    GqlRunningQuery {
      variables: HashMap::new(),
      fragments: HashMap::new(),
      fragment_fields: HashMap::new(),
      query_ast: doc,
      starting_type: "Query".to_owned(),
    }
  }

  #[inline(always)]
  fn get_queries<'a>(&'a self) -> Vec<&'a query::Query> {
    self
      .query_ast
      .definitions
      .iter()
      .filter_map(|d| match d {
        query::Definition::Operation(query::OperationDefinition::Query(q)) => Some(q),
        _ => None,
      })
      .collect()
  }

  #[inline(always)]
  fn get_mutations<'a>(&'a self) -> Vec<&'a query::Mutation> {
    self
      .query_ast
      .definitions
      .iter()
      .filter_map(|d| match d {
        query::Definition::Operation(query::OperationDefinition::Mutation(q)) => Some(q),
        _ => None,
      })
      .collect()
  }

  #[inline(always)]
  fn get_subscriptions<'a>(&'a self) -> Vec<&'a query::Subscription> {
    self
      .query_ast
      .definitions
      .iter()
      .filter_map(|s| match s {
        query::Definition::Operation(query::OperationDefinition::Subscription(s)) => Some(s),
        _ => None,
      })
      .collect()
  }

  pub fn parse_fragments(&mut self) -> GqlExecResult<()> {
    for item in &self.query_ast.definitions {
      match item {
        query::Definition::Fragment(fragment) => {
          self
            .fragments
            .insert(fragment.name.clone(), fragment.to_owned());
        }
        _ => {}
      }
    }
    Ok(())
  }

  pub fn get_var_defs(&self) -> Vec<&query::VariableDefinition> {
    self
      .query_ast
      .definitions
      .iter()
      .filter_map(|d| -> Option<&[query::VariableDefinition]> {
        match d {
          query::Definition::Operation(query::OperationDefinition::Query(q)) => {
            Some(&q.variable_definitions)
          }
          query::Definition::Operation(query::OperationDefinition::Mutation(m)) => {
            Some(&m.variable_definitions)
          }
          query::Definition::Operation(query::OperationDefinition::Subscription(s)) => {
            Some(&s.variable_definitions)
          }
          _ => None,
        }
      })
      .flatten()
      .collect()
  }

  pub fn parse_variables(&mut self, var_values: Option<JsonValue>) -> GqlExecResult<()> {
    // A map of the names and definitions of variables
    // defined in the top of the query
    let mut var_defs = HashMap::new();
    for var_def in self.get_var_defs() {
      var_defs.insert(var_def.name.to_owned(), var_def);
    }

    // if there are no variables defined, we can bail out early
    if var_defs.len() == 0 && var_values.is_none() {
      return Ok(());
    }

    let var_value_map: JsonMap<String, JsonValue> = {
      match var_values {
        Some(JsonValue::Object(vmap)) => Ok(vmap),
        None => Ok(JsonMap::new()),
        Some(v) => Err(GqlQueryErr::Variable(QueryValidationError::new(
          format!("Variables object in reqest was of the wrong type."),
          format!("{:?}", v),
        ))),
      }
    }?;

    let mut variables = HashMap::with_capacity(var_defs.len());
    // For each variable with a provided value,
    // match it with its definition
    // and then assign it to the internal map
    for (var_name, var_value) in var_value_map.into_iter() {
      let gql_var_value = json_to_gql(var_value);
      let var_def =
        var_defs
          .get(&var_name)
          .ok_or(GqlQueryErr::Variable(QueryValidationError::new(
            format!("Unexpected variable {} found", &var_name),
            var_name.clone(),
          )))?;
      if !naive_check_var_type(&var_def.var_type, &gql_var_value) {
        return Err(GqlQueryErr::Variable(QueryValidationError::new(
          format!(
            "the variable {} was not of type {:?}",
            var_name, &var_def.var_type
          ),
          var_name.clone(),
        )));
      }
      var_defs.remove(&var_name);
      variables.insert(var_name, gql_var_value);
    }

    // any variables that did not have provided values must have default values
    for (var_name, var_def) in var_defs.iter() {
      if let Some(default) = &var_def.default_value {
        variables.insert(var_name.to_owned(), default.to_owned());
      } else {
        return Err(GqlQueryErr::Variable(QueryValidationError::new(
          format!("Variable {} was not provided a value", var_name),
          Default::default(),
        )));
      }
    }
    self.variables = variables;
    Ok(())
  }

  pub fn get_fields(
    &self,
    selection: query::Selection,
    on_type: &str,
  ) -> GqlExecResult<Vec<query::Field>> {
    match selection {
      query::Selection::Field(f) => Ok(vec![f]),
      query::Selection::FragmentSpread(spread) => {
        let mut fields = Vec::new();
        let fragment = self
          .fragments
          .get(&spread.fragment_name)
          .ok_or(GqlQueryErr::Fragment(QueryValidationError::new(
            format!("Fragment {} not found", &spread.fragment_name),
            spread.fragment_name,
          )))?;
        for s in &fragment.selection_set.items {
          fields.extend(self.get_fields(s.to_owned(), on_type)?);
        }
        Ok(fields)
      }
      query::Selection::InlineFragment(inline) => {
        if let Some(query::TypeCondition::On(type_name)) = inline.type_condition {
          if type_name == on_type {
            return Ok(Vec::new());
          }
        }
        let mut fields = Vec::new();
        for s in inline.selection_set.items {
          fields.extend(self.get_fields(s, on_type)?);
        }
        Ok(fields)
      }
    }
  }

  pub fn fields_from_selectionset(
    &self,
    set: &query::SelectionSet,
    on_type: &str,
  ) -> GqlExecResult<Vec<query::Field>> {
    Ok(
      set
        .items
        .iter()
        .map(|e| self.get_fields(e.clone(), on_type))
        .collect::<GqlExecResult<Vec<Vec<query::Field>>>>()?
        .into_iter()
        .flatten()
        .collect(),
    )
  }

  //fn parse_fields_selection

  pub fn get_initial_items(&mut self) -> GqlExecResult<Vec<FieldSelection>> {
    let queries = self.get_queries();
    let mutations = self.get_mutations();
    let subscriptions = self.get_subscriptions();
    // Need to make sure we are only handling one type at a time
    if queries.len() > 0 && mutations.len() == 0 && subscriptions.len() == 0 {
      let res = Ok(
        queries
          .iter()
          .map(|q| match q.directives.len() > 0 {
            true => Err(GqlQueryErr::Directive(QueryValidationError::new(
              format!("No directives supported on query"),
              "Query".to_owned(),
            ))),
            false => Ok(FieldSelection::new(
              q.name.clone(),
              self.fields_from_selectionset(&q.selection_set, "Query")?,
            )),
          })
          .collect::<GqlExecResult<Vec<FieldSelection>>>()?,
      );
      self.starting_type = "Query".to_owned();
      return res;
    } else if mutations.len() > 0 && queries.len() == 0 && subscriptions.len() == 0 {
      let res = Ok(
        mutations
          .iter()
          .map(|q| match q.directives.len() > 0 {
            true => Err(GqlQueryErr::Directive(QueryValidationError::new(
              format!("No directives supported on a mutation"),
              "Query".to_owned(),
            ))),
            false => Ok(FieldSelection::new(
              q.name.clone(),
              self.fields_from_selectionset(&q.selection_set, "Mutation")?,
            )),
          })
          .collect::<GqlExecResult<Vec<FieldSelection>>>()?,
      );
      self.starting_type = "Mutation".to_owned();
      return res;
    } else if subscriptions.len() > 0 && queries.len() == 0 && mutations.len() == 0 {
      let res = Ok(
        subscriptions
          .iter()
          .map(|q| match q.directives.len() > 0 {
            true => Err(GqlQueryErr::Directive(QueryValidationError::new(
              format!("No directives supported on subscription"),
              "Query".to_owned(),
            ))),
            false => Ok(FieldSelection::new(
              q.name.clone(),
              self.fields_from_selectionset(&q.selection_set, "Subscription")?,
            )),
          })
          .collect::<GqlExecResult<Vec<FieldSelection>>>()?,
      );
      self.starting_type = "Subscription".to_owned();
      return res;
    }
    Err(GqlQueryErr::Field(QueryValidationError::new(
      format!("Request may only contain"),
      "Query".to_owned(),
    )))
  }
}

pub fn json_to_gql(value: JsonValue) -> GqlValue {
  match value {
    JsonValue::Null => GqlValue::Null,
    JsonValue::Bool(b) => GqlValue::Boolean(b),
    JsonValue::Number(n) => {
      if n.is_u64() || n.is_i64() {
        let f = n.as_i64().unwrap();
        GqlValue::Int(query::Number::from(f as i32))
      } else {
        GqlValue::Float(n.as_f64().unwrap())
      }
    }
    JsonValue::String(s) => GqlValue::String(s),
    JsonValue::Array(a) => GqlValue::List(a.iter().map(|i| json_to_gql(i.to_owned())).collect()),
    JsonValue::Object(o) => {
      let mut bmap = BTreeMap::new();
      for (key, val) in o.into_iter() {
        bmap.insert(key.to_owned(), json_to_gql(val));
      }
      GqlValue::Object(bmap)
    }
  }
}

pub fn gql_to_json(value: GqlValue) -> GqlExecResult<JsonValue> {
  match value {
    GqlValue::Null => Ok(JsonValue::Null),
    GqlValue::Boolean(b) => Ok(json!(b)),
    GqlValue::Float(f) => Ok(JsonValue::Number(JsonNumber::from_f64(f).unwrap())),
    GqlValue::Int(i) => Ok(json!(i.as_i64().unwrap())),
    GqlValue::String(s) => Ok(json!(s)),
    GqlValue::Enum(n) => Ok(json!(n)),
    GqlValue::Variable(v) => Err(GqlQueryErr::Variable(QueryValidationError::new(
      "Could not turn graphql varaible into JSON".to_owned(),
      v,
    ))),
    GqlValue::Object(o) => {
      let mut map = JsonMap::new();
      for (key, val) in o.iter() {
        map.insert(key.to_owned(), gql_to_json(val.to_owned())?);
      }
      Ok(JsonValue::Object(map))
    }
    GqlValue::List(l) => {
      let mut items = Vec::new();
      for il in l {
        items.push(gql_to_json(il)?);
      }
      Ok(JsonValue::Array(items))
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use graphql_parser::parse_query;
  use serde_json::from_str;

  #[test]
  fn test_loading_fragments() {
    let fragmenty_query = include_str!("../../tests/fragments.graphql");
    let q_ast = parse_query(fragmenty_query).unwrap();

    let mut exec = GqlRunningQuery::new(q_ast);
    exec.parse_fragments().unwrap();
    // good change to check what happens when there are no variable definitions
    exec
      .parse_variables(Some(JsonValue::Object(JsonMap::new())))
      .unwrap();
    assert_eq!(exec.fragments.len(), 3);
  }

  #[test]
  fn test_loading_variables() {
    let var_data = include_str!("../../tests/many_variables.json");
    let req: GqlRequest = from_str(var_data).unwrap();
    let q_ast = parse_query(&req.query).unwrap();
    let mut exec = GqlRunningQuery::new(q_ast);
    exec.parse_variables(req.variables).unwrap();
    assert_eq!(
      exec.variables.get("foo").unwrap(),
      &GqlValue::String("Hello".to_owned())
    );
    assert_eq!(
      exec.variables.get("bar"),
      Some(&GqlValue::Int(query::Number::from(6)))
    );
    assert_eq!(exec.variables.get("sham"), Some(&GqlValue::Boolean(true)));
  }

  #[test]
  fn test_invalid_variables() {
    let request_json = include_str!("../../tests/bad_variables.json");
    let request: GqlRequest = from_str(request_json).unwrap();
    let q_ast = parse_query(&request.query).unwrap();
    let mut exec = GqlRunningQuery::new(q_ast);
    let var_parse_result = exec.parse_variables(request.variables);
    assert!(var_parse_result.is_err());
  }

  #[test]
  fn parse_subscription() {
    let mut exec = GqlRunningQuery::new(
      parse_query(
        r#"
      subscription {
        newMessage {
          id
          content
        }
      }
    "#,
      )
      .unwrap(),
    );
    let start_fields = exec.get_initial_items().unwrap();
    assert_eq!(start_fields.len(), 1);
  }
}
