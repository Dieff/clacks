use crate::auth;
use crate::config;
use crate::gql_context::GqlContext;
use crate::gqln::{GqlRequest, GqlResponse, GqlSchema};
use crate::models::*;
use crate::ws_actors::*;
use actix::Addr;
use actix_web::{http::StatusCode, web, Error, HttpRequest, HttpResponse, Responder};
use actix_web_actors::ws;
use diesel::mysql::MysqlConnection;
use log::info;
use serde;
use serde::{Deserialize, Serialize};
use std::fmt;
// TODO: Make this into impl REsponder
use diesel::result::Error as DBError;

#[derive(Debug)]
pub struct DbQueryErr(DBError);

impl std::convert::From<DBError> for DbQueryErr {
  fn from(err: DBError) -> Self {
    DbQueryErr(err)
  }
}

impl std::fmt::Display for DbQueryErr {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{:?}", self.0)
  }
}

impl actix_web::ResponseError for DbQueryErr {}

#[derive(Clone)]
pub struct ApiContext {
  pub db: DbPool,
  pub config: config::AppConfig,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ApiChannel {
  id: i32,
  display_name: Option<String>,
}

pub struct Channels(Vec<ApiChannel>);

impl Responder for Channels {
  type Error = DbQueryErr;
  type Future = Result<HttpResponse, DbQueryErr>;

  fn respond_to(self, _req: &HttpRequest) -> Self::Future {
    Ok(
      HttpResponse::Ok()
        .content_type("application/json")
        .json(&self.0),
    )
  }
}

pub fn r_get_channels(context: web::Data<ApiContext>) -> Result<Channels, DbQueryErr> {
  let channels = get_channels(&context.db.get().unwrap())?;
  Ok(Channels(
    channels
      .into_iter()
      .map(|ch| ApiChannel {
        id: ch.id,
        display_name: ch.display_name,
      })
      .collect(),
  ))
}

pub fn r_get_jwt(path: web::Path<(String,)>, context: web::Data<ApiContext>) -> String {
  let name = "bob";
  auth::encode_jwt(&path.0, name, &context.config.jwt_secret.as_ref().unwrap())
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelInput {
  display_name: String,
  initial_users: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreateChannelOutput {
  id: i32,
}

impl Responder for CreateChannelOutput {
  type Error = DbQueryErr;
  type Future = Result<HttpResponse, DbQueryErr>;

  fn respond_to(self, _req: &HttpRequest) -> Self::Future {
    Ok(HttpResponse::build(StatusCode::OK).json(self))
  }
}

pub fn r_create_channel(
  channel: web::Json<CreateChannelInput>,
  context: web::Data<ApiContext>,
) -> Result<CreateChannelOutput, DbQueryErr> {
  let conn: &MysqlConnection = &context.db.get().unwrap();
  let new_channel = create_channel(conn, &channel.display_name)?;
  for user in &channel.initial_users {
    add_user_to_channel(conn, user, new_channel.id, "member")?;
  }
  Ok(CreateChannelOutput { id: new_channel.id })
}

pub fn r_remove_user(
  path: web::Path<(i32, String)>,
  context: web::Data<ApiContext>,
) -> Result<HttpResponse, DbQueryErr> {
  let conn: &MysqlConnection = &context.db.get().unwrap();
  remove_user(conn, path.0, &path.1)?;
  Ok(HttpResponse::Ok().finish())
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct ChannelInfo {
  display_name: String,
  users: Vec<String>,
}

pub fn r_get_channel_info(
  path: web::Path<(i32,)>,
  context: web::Data<ApiContext>,
) -> Result<HttpResponse, DbQueryErr> {
  let conn: &MysqlConnection = &context.db.get().unwrap();
  match get_channel(conn, path.0)? {
    Some(ch) => {
      let users = get_channel_users(conn, path.0)?;
      Ok(HttpResponse::Ok().json(ChannelInfo {
        display_name: ch.display_name.unwrap(),
        users,
      }))
    }
    None => Ok(HttpResponse::build(StatusCode::NOT_FOUND).finish()),
  }
}

pub struct ChannelUsers(Vec<String>);

impl Responder for ChannelUsers {
  type Error = Error;
  type Future = Result<HttpResponse, Error>;

  fn respond_to(self, _req: &HttpRequest) -> Self::Future {
    let body = serde_json::to_string(&self.0)?;
    Ok(
      HttpResponse::Ok()
        .content_type("application/json")
        .body(body),
    )
  }
}

pub struct ApiChannelUsers(Vec<String>);

impl Responder for ApiChannelUsers {
  type Error = DbQueryErr;
  type Future = Result<HttpResponse, DbQueryErr>;

  fn respond_to(self, _req: &HttpRequest) -> Self::Future {
    Ok(HttpResponse::Ok().json(&self.0))
  }
}

pub fn r_get_channel_users(
  path: web::Path<(i32,)>,
  context: web::Data<ApiContext>,
) -> Result<ApiChannelUsers, DbQueryErr> {
  let conn: &MysqlConnection = &context.db.get().unwrap();
  Ok(ApiChannelUsers(get_channel_users(conn, path.0)?))
}

#[derive(Deserialize, Debug)]
pub struct ApiAddUser {
  uid: String,
}

pub fn r_add_user(
  path: web::Path<(i32,)>,
  data: web::Json<ApiAddUser>,
  context: web::Data<ApiContext>,
) -> Result<HttpResponse, DbQueryErr> {
  let conn: &MysqlConnection = &context.db.get().unwrap();
  add_user_to_channel(conn, &data.uid, path.0, "temp")?;
  Ok(HttpResponse::Ok().finish())
}

pub fn r_delete_channel(
  path: web::Path<(i32,)>,
  context: web::Data<ApiContext>,
) -> Result<HttpResponse, DbQueryErr> {
  let conn: &MysqlConnection = &context.db.get().unwrap();
  delete_channel(conn, path.0)?;
  Ok(HttpResponse::Ok().finish())
}

// For standard health checks
pub fn r_health() -> impl Responder {
  HttpResponse::Ok()
}

// ---------------------------- Graphql Routes -----------------------------------

// When a new websocket request comes in, start a new actor
pub fn r_wsspawn(
  req: HttpRequest,
  stream: web::Payload,
  recip: web::Data<Addr<ConnectionTracker>>,
  config: web::Data<config::AppConfig>,
) -> Result<HttpResponse, Error> {
  info!("New websocket request. Some subscriptions will be next.");
  let id = match req.headers().get("Authorization").map(|i| i.to_str()) {
    Some(Ok(s)) => match auth::decode_jwt(s, &config.jwt_secret.as_ref().unwrap()) {
      Ok(claims) => Some(claims.id),
      _ => None,
    },
    _ => None,
  };

  let handler = WsHandler::new(
    recip.get_ref().to_owned(),
    id,
    config.jwt_secret.clone().unwrap(),
  );
  ws::start_with_protocols(handler, &["graphql-ws"], &req, stream)
}

#[derive(Clone)]
pub struct GqlRouteContext {
  db: DbPool,
  schema: GqlSchema<GqlContext>,
}

impl GqlRouteContext {
  pub fn new(schema: GqlSchema<GqlContext>, db: DbPool) -> Self {
    GqlRouteContext { db, schema }
  }
}

pub fn handle_graphql_req(
  req: &HttpRequest,
  payload: GqlRequest,
  ctx: &web::Data<GqlRouteContext>,
  tracker: &Addr<ConnectionTracker>,
  config: &config::AppConfig,
) -> HttpResponse {
  if let Some(auth_header) = req.headers().get("Authorization") {
    if let Ok(jwt) = auth_header.to_str() {
      if let Ok(user_info) = auth::decode_jwt(jwt, &config.jwt_secret.as_ref().unwrap()) {
        let mut context = GqlContext::new(ctx.db.clone(), user_info.id, tracker.to_owned());
        let gql_resp = ctx.schema.resolve(&mut context, payload, None);
        return HttpResponse::Ok().json(GqlResponse::from(gql_resp));
      }
    }
  }
  HttpResponse::Unauthorized().finish()
}

// The main POST endpoint for graphql queries
// such as reading data, sending messages
pub fn r_graphql_post(
  req: HttpRequest,
  payload: web::Json<GqlRequest>,
  gql_ctx: web::Data<GqlRouteContext>,
  tracker: web::Data<Addr<ConnectionTracker>>,
  config: web::Data<config::AppConfig>,
) -> impl Responder {
  handle_graphql_req(
    &req,
    payload.0,
    &gql_ctx,
    tracker.get_ref(),
    config.get_ref(),
  )
}

// graphql is also supposed to be able to handle GET requests
pub fn r_graphql_get(
  req: HttpRequest,
  payload: web::Query<GqlRequest>,
  gql_ctx: web::Data<GqlRouteContext>,
  tracker: web::Data<Addr<ConnectionTracker>>,
  config: web::Data<config::AppConfig>,
) -> impl Responder {
  handle_graphql_req(
    &req,
    payload.0,
    &gql_ctx,
    tracker.get_ref(),
    config.get_ref(),
  )
}
