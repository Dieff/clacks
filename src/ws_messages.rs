use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::gqln::GqlRequest;

#[derive(Serialize, Debug, PartialEq, Clone)]
pub enum WsError {
  MessageParse(String),
  MessageEncode(String),
  Unauthorized,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SubDataPayload {
  pub data: Value,
  pub errors: Vec<Value>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SubData {
  pub payload: SubDataPayload,
  pub id: String,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ServerWsMessage {
  ConnectionAck,
  ConnectionError,
  KA,
  Data(SubData),
  #[serde(rename = "error")]
  GqlError(WsError),
  Complete,
}

impl ServerWsMessage {
  pub fn from_err(err: WsError) -> Self {
    Self::GqlError(err)
  }
  pub fn ack() -> Self {
    Self::ConnectionAck
  }
  pub fn data(id: String, data: Value) -> Self {
    Self::Data(SubData {
      id,
      payload: SubDataPayload {
        data,
        errors: Vec::new(),
      },
    })
  }
}

impl std::convert::From<WsError> for ServerWsMessage {
  fn from(msg: WsError) -> Self {
    Self::GqlError(msg)
  }
}

impl std::convert::From<&ServerWsMessage> for String {
  fn from(msg: &ServerWsMessage) -> Self {
    match serde_json::to_string(msg) {
      Ok(s) => s,
      Err(_) => {
        let e_msg = ServerWsMessage::from_err(WsError::MessageEncode(format!(
          "Could not encode result as JSON"
        )));
        serde_json::to_string(&e_msg).unwrap()
      }
    }
  }
}

#[derive(Deserialize, Debug, PartialEq, Clone)]
pub struct ClientInit {
  pub payload: Map<String, Value>,
}

#[derive(Deserialize, Debug, PartialEq, Clone)]
pub struct ClientStart {
  pub id: String,
  pub payload: GqlRequest,
}

#[derive(Deserialize, Debug, PartialEq, Clone)]
pub struct ClientStop {
  pub id: String,
}

#[derive(Deserialize, Debug, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ClientWsMessage {
  ConnectionInit(ClientInit),
  Start(ClientStart),
  Stop(ClientStop),
  ConnectionTerminate,
}

impl ClientWsMessage {
  pub fn from_str(msg: &str) -> Result<Self, WsError> {
    serde_json::from_str(msg).map_err(|e| {
      WsError::MessageParse(format!(
        "Error parsing json message: line {}, col {}",
        e.line(),
        e.column()
      ))
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn deserialize_client_message() {
    let init_message = r#"
      {
        "type": "connection_init",
        "payload": {}
      }
    "#;
    let init: ClientWsMessage = serde_json::from_str(init_message).unwrap();
    assert_eq!(
      init,
      ClientWsMessage::ConnectionInit(ClientInit {
        payload: Map::new()
      })
    );
    let start_message = r#"
      {
        "id":"1",
        "type":"start",
        "payload": {
          "variables": {},
          "extensions":{},
          "operationName":null,
          "query":"subscription {\n  Message {\n    node {\n      name\n    }\n  }\n}\n"}
      }
    "#;
    let start: ClientWsMessage = serde_json::from_str(start_message).unwrap();
    if let ClientWsMessage::Start(start) = start {
      assert_eq!(start.id, "1".to_owned());
    } else {
      panic!()
    };
  }
}
