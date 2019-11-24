use crate::gqln::{GqlRequest, ResolutionErr};
use crate::ws_actors::WsHandler;
use actix::{Addr, Message};
use serde_json::{json, Value as JsonValue};

#[derive(Message)]
pub struct MsgNewSubscription {
  pub user_id: String,
  pub sub_id: String,
  pub sub: GqlRequest,
  pub addr: Addr<WsHandler>,
}

#[derive(Message)]
pub struct MsgWsDisconnected {
  pub id: String,
}

#[derive(Message, Clone, Debug)]
pub struct MsgSubscriptionStop {
  pub sub_id: String,
  pub user_id: String,
}

#[derive(Message, Clone, Debug)]
pub struct MsgMessageCreated {
  pub channel: i32,
  pub content: String,
  pub sender: String,
  pub msg_id: i32,
}

impl MsgMessageCreated {
  pub fn new(channel: i32, content: String, sender: String, msg_id: i32) -> Self {
    MsgMessageCreated {
      channel,
      content,
      sender,
      msg_id,
    }
  }
}

#[derive(Message, Clone, Debug)]
pub struct MsgSubscriptionData {
  pub errors: Vec<JsonValue>,
  pub data: Option<JsonValue>,
  pub id: String,
}

impl MsgSubscriptionData {
  pub fn new(id: String, result: Result<JsonValue, ResolutionErr>) -> Self {
    match result {
      Ok(data) => MsgSubscriptionData {
        errors: Vec::new(),
        data: Some(data),
        id,
      },
      Err(err) => MsgSubscriptionData {
        errors: vec![json!(err)],
        data: None,
        id,
      },
    }
  }
}
