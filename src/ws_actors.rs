use actix::{Actor, ActorContext, Addr, AsyncContext, Context, Handler, StreamHandler};
use actix_web_actors::ws;
use graphql_parser::query::Value as GqlValue;
use log::info;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use crate::auth;
use crate::gql_context::{GqlContext, Schema};
use crate::gqln::{GqlRequest, GqlRoot};
use crate::models::DbPool;
use crate::ws_messages::{ClientWsMessage, ServerWsMessage, WsError};

// --------------- Messages -----------------------
mod messages;
pub use messages::*;

// -------------- Actors and Types ----------------

struct SubscriptionSource {
  addr: Addr<WsHandler>,
  req: GqlRequest,
}

pub struct ConnectionTracker {
  pub connections: usize,
  subs: HashMap<String, SubscriptionSource>,
  schema: Schema,
  pool: DbPool,
}

impl ConnectionTracker {
  pub fn new(schema: Schema, pool: DbPool) -> Self {
    ConnectionTracker {
      connections: 0,
      subs: HashMap::new(),
      schema,
      pool,
    }
  }
}

impl Actor for ConnectionTracker {
  type Context = Context<Self>;
}

impl Handler<MsgNewSubscription> for ConnectionTracker {
  type Result = ();

  fn handle(&mut self, msg: MsgNewSubscription, ctx: &mut Self::Context) {
    self.connections += 1;
    self.subs.insert(
      msg.user_id,
      SubscriptionSource {
        addr: msg.addr,
        req: msg.sub,
      },
    );
    println!("{} clients are connected", self.connections);
  }
}

impl Handler<MsgWsDisconnected> for ConnectionTracker {
  type Result = ();

  fn handle(&mut self, msg: MsgWsDisconnected, ctx: &mut Self::Context) {
    self.connections = self.connections.saturating_sub(1);
    println!("{} clients are connected", self.connections);
  }
}

impl Handler<MsgMessageCreated> for ConnectionTracker {
  type Result = ();

  fn handle(&mut self, msg: MsgMessageCreated, ctx: &mut Self::Context) {
    for sub in self.subs.values() {
      let mut context = GqlContext::new(self.pool.clone(), "0".to_owned(), ctx.address());
      let mut root = GqlRoot::new();
      root.insert("id".to_owned(), GqlValue::String("test".to_owned()));
      root.insert(
        "content".to_owned(),
        GqlValue::String(msg.content.to_owned()),
      );
      let res = self
        .schema
        .resolve(&mut context, sub.req.clone(), Some(root));
      sub
        .addr
        .do_send(MsgSubscriptionData::new("1".to_owned(), res));
    }
  }
}

pub struct WsHandler {
  conn_id: Option<String>,
  secret: String,
  tracker: Addr<ConnectionTracker>,
  subscriptions: Vec<String>,
}

impl WsHandler {
  pub fn new(tracker: Addr<ConnectionTracker>, id: Option<String>, secret: String) -> Self {
    WsHandler {
      conn_id: id,
      tracker,
      secret,
      subscriptions: Vec::new(),
    }
  }

  fn disconnected(&self) {
    if let Some(id) = &self.conn_id {
      self.tracker.do_send(MsgWsDisconnected {
        id: id.clone(),
        subscriptions: self.subscriptions.clone(),
      });
    }
  }
}

impl Actor for WsHandler {
  type Context = ws::WebsocketContext<Self>;
}

impl StreamHandler<ws::Message, ws::ProtocolError> for WsHandler {
  fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
    info!("recieved a websocket message {:?}", msg);
    match msg {
      ws::Message::Ping(msg) => ctx.pong(&msg),
      ws::Message::Text(text) => match ClientWsMessage::from_str(&text) {
        Err(e) => ctx.text(&ServerWsMessage::from_err(e)),
        Ok(ClientWsMessage::ConnectionInit(init)) => {
          if let Some(JsonValue::String(jwt)) = init.payload.get("Authorization") {
            match auth::decode_jwt(jwt, &self.secret) {
              Ok(user_info) => {
                info!(
                  "A user has sent auth over websocket. They are: {}",
                  user_info.id
                );
                self.conn_id = Some(user_info.id);
              }
              Err(e) => {
                info!("JWT Error in websocket {:?}", e);
                self.disconnected();
                ctx.close(None);
                ctx.stop();
              }
            }
          }
          ctx.text(&ServerWsMessage::ack());
        }
        Ok(ClientWsMessage::ConnectionTerminate) => {
          ctx.close(None);
          self.disconnected();
          ctx.stop();
        }
        Ok(ClientWsMessage::Start(new_sub)) => {
          if let Some(id) = &self.conn_id {
            dbg!("REgistering a new subscription!");
            self.tracker.do_send(MsgNewSubscription {
              user_id: id.clone(),
              sub_id: new_sub.id,
              addr: ctx.address(),
              sub: new_sub.payload,
            });
            info!("New subscription");
          } else {
            ctx.text(&ServerWsMessage::from(WsError::Unauthorized));
          }
          // new sub!
        }
        Ok(ClientWsMessage::Stop(end_sub)) => {
          // end sub!
        }
      },
      ws::Message::Close(_) => {
        info!("client has disconnected");
        self.disconnected();
        // End the actor
        ctx.stop();
      }
      _ => (),
    }
  }
}

impl Handler<MsgSubscriptionData> for WsHandler {
  type Result = ();
  fn handle(&mut self, data: MsgSubscriptionData, ctx: &mut Self::Context) {
    if let Some(jdata) = data.data {
      if data.errors.len() == 0 {
        let resp = ServerWsMessage::data(data.id, jdata);
        ctx.text(&resp);
      }
    }
    // TODO: handle error condition
  }
}
