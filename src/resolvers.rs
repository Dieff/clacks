use graphql_parser::query;
use std::collections::BTreeMap;

use crate::gql_context::GqlContext;
use crate::gqln::{
  GqlArgs, GqlObj, GqlRoot, GqlSchema, MissingArgument, ResResult, ResolutionErr, ResolutionReturn,
};
use crate::models::create_message;
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

pub fn mutation_create_message(
  _root: &GqlRoot,
  args: GqlArgs,
  context: &mut GqlContext,
  _schema: &GqlSchema<GqlContext>,
) -> ResResult {
  let input_err =
    ResolutionErr::MissingArgument(MissingArgument::new("Mutation", "createMessage", "input"));
  let input =
    assert_arg_is_object(args.get("input").ok_or(input_err.clone())?).ok_or(input_err.clone())?;
  let msg_content = assert_arg_is_string(input.get("content").unwrap_or(&query::Value::Null))
    .ok_or(ResolutionErr::MissingArgument(MissingArgument::new(
      "CreateMessageInput",
      "content",
      "",
    )))?
    .to_owned();
  let msg_channel = 0;
  let actor_message = MsgMessageCreated::new(msg_channel, msg_content, vec![0, 0]);
  context.ws_addr.do_send(actor_message);
  let mut bmap = GqlObj::new();
  bmap.insert("id".to_owned(), query::Value::String("asdf".to_owned()));
  Ok(ResolutionReturn::Type(("Message".to_owned(), bmap)))
}

pub fn subscription_message(
  root: &GqlRoot,
  args: GqlArgs,
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
