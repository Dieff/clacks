use diesel::prelude::*;
use diesel::{mysql::MysqlConnection, r2d2::Error as DbConnsErr};
use graphql_parser::query;
use log::info;
use std::collections::BTreeMap;
use std::convert::TryInto;

use crate::gql_context::GqlContext;
use crate::gqln::{
  GqlArgs, GqlObj, GqlRoot, GqlSchema, MissingArgument, ResResult, ResolutionErr, ResolutionReturn,
};
use crate::models::*;
use crate::ws_actors::MsgMessageCreated;

fn assert_arg_is_object<'a>(arg: &'a query::Value) -> Option<&'a GqlObj> {
  match arg {
    query::Value::Object(o) => Some(o),
    _ => None,
  }
}

fn assert_arg_is_string(arg: &query::Value) -> Option<&str> {
  match arg {
    query::Value::String(s) => Some(s),
    _ => None,
  }
}

fn assert_arg_is_number(arg: &query::Value) -> Option<i32> {
  match arg {
    query::Value::Int(i) => {
      if let Some(n) = i.as_i64() {
        if let Ok(m) = n.try_into() {
          return Some(m);
        }
      }
      None
    }
    _ => None,
  }
}

fn assert_has_id(root: &GqlRoot) -> Result<String, ResolutionErr> {
  let id = root
    .get("id")
    .ok_or(ResolutionErr::new_invalid_field("_", "id"))?;
  Ok(
    assert_arg_is_string(id)
      .ok_or(ResolutionErr::new_invalid_field("_", "id"))?
      .to_owned(),
  )
}

impl From<r2d2::Error> for ResolutionErr {
  fn from(_: r2d2::Error) -> Self {
    Self::io_err("Timeout while waiting for database connections")
  }
}

impl From<diesel::result::Error> for ResolutionErr {
  fn from(e: diesel::result::Error) -> Self {
    ResolutionErr::io_err(&format!("{:?}", e))
  }
}

pub fn mutation_create_message(
  _root: &GqlRoot,
  args: GqlArgs,
  context: &mut GqlContext,
  _schema: &GqlSchema<GqlContext>,
) -> ResResult {
  let input_err = ResolutionErr::new_missing_argument("Mutation", "createMessage", "input");
  let input =
    assert_arg_is_object(args.get("input").ok_or(input_err.clone())?).ok_or(input_err.clone())?;
  let msg_content = assert_arg_is_string(input.get("content").unwrap_or(&query::Value::Null))
    .ok_or(ResolutionErr::MissingArgument(MissingArgument::new(
      "CreateMessageInput",
      "content",
      "",
    )))?
    .to_owned();
  let msg_channel = assert_arg_is_number(input.get("channel").ok_or(input_err.clone())?)
    .ok_or(input_err.clone())?;

  let conn: &MysqlConnection = &*context.db.get()?;
  let new_msg = create_message(&conn, &context.cur_user, msg_channel, &msg_content)
    .map_err(|_| ResolutionErr::io_err("Database error"))?;

  let actor_message = MsgMessageCreated::new(
    msg_channel,
    msg_content.clone(),
    context.cur_user.clone(),
    100,
  );
  context.ws_addr.do_send(actor_message);

  let mut bmap = GqlObj::new();
  bmap.insert(
    "id".to_owned(),
    query::Value::Int(query::Number::from(new_msg.id.to_owned())),
  );
  Ok(ResolutionReturn::Type(("Message".to_owned(), bmap)))
}

pub fn subscription_message(
  root: &GqlRoot,
  _: GqlArgs,
  context: &mut GqlContext,
  _schema: &GqlSchema<GqlContext>,
) -> ResResult {
  let mut bmap = BTreeMap::new();
  if let Some(id) = root.get("id") {
    bmap.insert("id".to_owned(), id.to_owned());
  }
  if let Some(content) = root.get("content") {
    bmap.insert("content".to_owned(), content.to_owned());
  }
  Ok(ResolutionReturn::Type(("Message".to_owned(), bmap)))
}

pub fn query_me(
  _root: &GqlRoot,
  _args: GqlArgs,
  context: &mut GqlContext,
  _: &GqlSchema<GqlContext>,
) -> ResResult {
  Ok(ResolutionReturn::Scalar(query::Value::String(
    context.cur_user.to_owned(),
  )))
}

pub fn mutation_read_message(
  _root: &GqlRoot,
  args: GqlArgs,
  context: &mut GqlContext,
  _: &GqlSchema<GqlContext>,
) -> ResResult {
  let message_id = assert_arg_is_string(args.get("message").ok_or(
    ResolutionErr::new_missing_argument("Mutation", "readMessage", "message"),
  )?)
  .ok_or(ResolutionErr::new_missing_argument(
    "Mutation",
    "readMessage",
    "message",
  ))?;

  let msg: i32 = message_id
    .parse()
    .map_err(|_| ResolutionErr::new_missing_argument("Mutation", "readMessage", "message"))?;

  let conn: &MysqlConnection = &*context.db.get()?;
  mark_message_as_read(conn, msg, &context.cur_user)?;

  Ok(ResolutionReturn::Scalar(query::Value::Null))
}

pub fn query_unread(
  _root: &GqlRoot,
  _args: GqlArgs,
  context: &mut GqlContext,
  _: &GqlSchema<GqlContext>,
) -> ResResult {
  let conn: &MysqlConnection = &*context.db.get()?;
  let messages = get_unread(conn, &context.cur_user)?;
  Ok(ResolutionReturn::TypeList((
    "Message".to_owned(),
    messages
      .into_iter()
      .map(|id| {
        let mut bmap = BTreeMap::new();
        bmap.insert("id".to_owned(), query::Value::String(format!("{}", id)));
        bmap
      })
      .collect(),
  )))
}

pub fn message_sender(
  root: &GqlRoot,
  _args: GqlArgs,
  context: &mut GqlContext,
  _: &GqlSchema<GqlContext>,
) -> ResResult {
  let msg_id: i32 = assert_has_id(root)?.parse().unwrap();
  let conn: &MysqlConnection = &*context.db.get()?;
  let message = get_message(conn, msg_id)?.ok_or(ResolutionErr::QueryResult(format!(
    "Could not find message {}",
    msg_id
  )))?;
  Ok(ResolutionReturn::Scalar(query::Value::String(
    message.sender,
  )))
}
