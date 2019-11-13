use actix::Message;
use graphql_parser::query;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::fmt;

use crate::gqln::GqlSchema;

pub type GqlValue = query::Value;

/// represents the incoming JSON for a graphql request
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GqlRequest {
  pub query: String,
  pub operation_name: Option<String>,
  pub variables: Option<JsonValue>,
}

/// An error that is encountered with the schema or resolvers when setting up
/// a `GqlSchema`.
#[derive(Debug, Clone, Serialize)]
pub enum GqlSchemaErr {
  UknownScalar,
  DublicateDef,
  MissingType(String),
  MissingResolver((String, String)),
  InvalidResolver,
}

pub type SchemaResult<T> = Result<T, GqlSchemaErr>;

#[derive(Debug, Clone, Serialize)]
pub enum GqlQueryErr {
  Variable(QueryValidationError),
  Fragment(QueryValidationError),
  Directive(QueryValidationError),
  Field(QueryValidationError),
  Type(QueryValidationError),
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct QueryValidationError {
  msg: String,
  subject_name: String,
}

impl QueryValidationError {
  pub fn new(msg: String, subject_name: String) -> Self {
    QueryValidationError { msg, subject_name }
  }
}

pub type GqlExecResult<T> = Result<T, GqlQueryErr>;

#[derive(Debug, Clone, Serialize)]
pub struct MissingArgument {
  pub on_type: String,
  pub name: String,
  pub on_field: String,
}

impl MissingArgument {
  pub fn new(on_type: &str, on_field: &str, name: &str) -> Self {
    MissingArgument {
      on_type: on_type.to_owned(),
      on_field: on_field.to_owned(),
      name: name.to_owned(),
    }
  }
}

#[derive(Debug, Clone, Serialize)]
pub enum ResolutionErr {
  IO,
  QueryValidation(GqlQueryErr),
  SchemaIssue(GqlSchemaErr),
  QueryParseIssue(String),
  QueryResult(String),
  MissingArgument(MissingArgument),
}

impl ResolutionErr {
  pub fn new_invalid_field(on_type: &str, field: &str) -> Self {
    Self::QueryValidation(GqlQueryErr::Field(QueryValidationError::new(
      format!("Field {} was not found on type {}", field, on_type),
      field.to_owned(),
    )))
  }
  pub fn new_missing_resolver(on_type: &str, field: &str) -> Self {
    Self::SchemaIssue(GqlSchemaErr::MissingResolver((
      on_type.to_owned(),
      field.to_owned(),
    )))
  }
  pub fn new_missing_type(on_type: &str) -> Self {
    Self::SchemaIssue(GqlSchemaErr::MissingType(on_type.to_owned()))
  }
  pub fn new_missing_argument(on_type: &str, on_field: &str, arg_name: &str) -> Self {
    Self::MissingArgument(MissingArgument {
      on_type: on_type.to_owned(),
      on_field: on_field.to_owned(),
      name: arg_name.to_owned(),
    })
  }
}

impl std::convert::From<GqlQueryErr> for ResolutionErr {
  fn from(err: GqlQueryErr) -> Self {
    Self::QueryValidation(err)
  }
}

pub type GqlObj = BTreeMap<String, GqlValue>;

#[derive(Debug, Clone)]
pub enum ResolutionReturn {
  Scalar(query::Value),
  List(Vec<GqlValue>),
  Type((String, GqlObj)),
  TypeList((String, Vec<GqlObj>)),
}

pub type ResResult = Result<ResolutionReturn, ResolutionErr>;
pub type GqlRoot = BTreeMap<String, query::Value>;
pub type GqlArgs = BTreeMap<String, query::Value>;

pub type ResolverBoxed<C> = Box<fn(&GqlRoot, GqlArgs, &mut C, &GqlSchema<C>) -> ResResult>;

#[derive(Clone)]
pub struct Resolver<C> {
  pub resolve: ResolverBoxed<C>,
  pub field: String,
  pub on_type: String,
}

impl<C> Resolver<C> {
  pub fn new(resolve: ResolverBoxed<C>, on_type: &str, on_field: &str) -> Self {
    Resolver {
      resolve,
      on_type: on_type.to_owned(),
      field: on_field.to_owned(),
    }
  }
}

impl<C> fmt::Debug for Resolver<C> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Resolver: {}.field: {}", self.on_type, self.field)
  }
}

#[derive(Serialize, Message)]
pub struct GqlResponse {
  pub data: Option<JsonValue>,
  pub errors: Vec<ResolutionErr>,
}

impl From<Result<JsonValue, ResolutionErr>> for GqlResponse {
  fn from(res_result: Result<JsonValue, ResolutionErr>) -> Self {
    match res_result {
      Ok(d) => GqlResponse {
        data: Some(d),
        errors: vec![],
      },
      Err(e) => GqlResponse {
        data: None,
        errors: vec![e],
      },
    }
  }
}
