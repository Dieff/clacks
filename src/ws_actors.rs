use actix::{Actor, ActorContext, Addr, AsyncContext, Context, Handler, StreamHandler};
use actix_web_actors::ws;
use graphql_parser::query::Value as GqlValue;
use log::{info, warn};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use crate::auth;
use crate::gql_context::{GqlContext, Schema};
use crate::gqln::{GqlRequest, GqlRoot};
use crate::models::{get_users_channels, DbPool};
use crate::ws_messages::{ClientWsMessage, ServerWsMessage, WsError};

// --------------- Messages -----------------------
mod messages;
pub use messages::*;

// -------------- Actors and Types ----------------

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
struct SubscriptionInstance {
  user: String,
  id: String,
}

struct ActiveSubscription {
  channels: Vec<i32>,
  addr: Addr<WsHandler>,
  req: GqlRequest,
}

pub struct ConnectionTracker {
  pub connections: usize,
  subscriptions: HashMap<SubscriptionInstance, ActiveSubscription>,
  channels: HashMap<i32, Vec<SubscriptionInstance>>,
  schema: Schema,
  pool: DbPool,
}

impl ConnectionTracker {
  pub fn new(schema: Schema, pool: DbPool) -> Self {
    ConnectionTracker {
      connections: 0,
      subscriptions: HashMap::new(),
      channels: HashMap::new(),
      schema,
      pool,
    }
  }

  fn remove_sub(&mut self, user: &String, sub_id: &String) {
    let instance = SubscriptionInstance {
      user: user.to_owned(),
      id: sub_id.to_owned(),
    };
    if let Some(sub) = self.subscriptions.get(&instance) {
      for channel in &sub.channels {
        let chsub = self.channels.get_mut(channel).unwrap();
        for i in 0..chsub.len() {
          if chsub[i] == instance {
            chsub.swap_remove(i);
          }
        }
      }
    }
  }

  fn remove_user(&mut self, user: &String) {
    let ids: Vec<String> = self
      .subscriptions
      .keys()
      .filter(|k| k.user == *user)
      .map(|k| k.id.clone())
      .collect();

    for id in &ids {
      self.remove_sub(user, id);
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
    let instance = SubscriptionInstance {
      user: msg.user_id.clone(),
      id: msg.sub_id.clone(),
    };
    let channels =
      get_users_channels(&self.pool.get().unwrap(), &msg.user_id).unwrap_or(Vec::new());
    info!("new user connected, listening on channels {:?}", &channels);
    self.subscriptions.insert(
      instance.clone(),
      ActiveSubscription {
        channels: channels.clone(),
        addr: msg.addr.clone(),
        req: msg.sub.clone(),
      },
    );

    for channel in channels {
      match self.channels.get_mut(&channel) {
        Some(subs) => {
          subs.push(instance.clone());
        }
        None => {
          self.channels.insert(channel, vec![instance.clone()]);
        }
      }
    }
    println!("{} clients are connected", self.connections);
  }
}

impl Handler<MsgWsDisconnected> for ConnectionTracker {
  type Result = ();

  fn handle(&mut self, msg: MsgWsDisconnected, ctx: &mut Self::Context) {
    self.connections = self.connections.saturating_sub(1);
    self.remove_user(&msg.id);
    println!("{} clients are connected", self.connections);
  }
}

impl Handler<MsgMessageCreated> for ConnectionTracker {
  type Result = ();

  fn handle(&mut self, msg: MsgMessageCreated, ctx: &mut Self::Context) {
    if let Some(subs) = self.channels.get(&msg.channel) {
      let mut root = GqlRoot::new();
      root.insert("id".to_owned(), GqlValue::String(format!("{}", msg.msg_id)));
      root.insert(
        "content".to_owned(),
        GqlValue::String(msg.content.to_owned()),
      );
      for sub in subs {
        // No need to tell a user about the message they just sent
        if sub.user != msg.sender {
          let mut context = GqlContext::new(self.pool.clone(), sub.user.clone(), ctx.address());
          let sub_data = self.subscriptions.get(sub).unwrap();
          let res = self
            .schema
            .resolve(&mut context, sub_data.req.clone(), Some(root.clone()));
          sub_data
            .addr
            .do_send(MsgSubscriptionData::new(sub.id.clone(), res));
        }
      }
    }
  }
}

impl Handler<MsgSubscriptionStop> for ConnectionTracker {
  type Result = ();

  fn handle(&mut self, msg: MsgSubscriptionStop, _ctx: &mut Self::Context) {
    self.remove_sub(&msg.user_id, &msg.sub_id);
  }
}

pub struct WsHandler {
  conn_id: Option<String>,
  secret: String,
  tracker: Addr<ConnectionTracker>,
}

impl WsHandler {
  pub fn new(tracker: Addr<ConnectionTracker>, id: Option<String>, secret: String) -> Self {
    WsHandler {
      conn_id: id,
      tracker,
      secret,
    }
  }

  fn disconnected(&self) {
    if let Some(id) = &self.conn_id {
      self.tracker.do_send(MsgWsDisconnected { id: id.clone() });
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
        Err(e) => {
          warn!("{:?}", e);
          ctx.text(&ServerWsMessage::from_err(e));
        }
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
          if self.conn_id == None {
            warn!("No authentication for client. Closing socket.");
            ctx.close(None);
            self.disconnected();
            ctx.stop();
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
            dbg!("REgistering a new subscription for user {}", &id);
            self.tracker.do_send(MsgNewSubscription {
              user_id: id.clone(),
              sub_id: new_sub.id,
              addr: ctx.address(),
              sub: new_sub.payload,
            });
            info!("New subscription");
          } else {
            warn!("Client attempted to subscribe without authorization");
            ctx.close(None);
          }
        }
        Ok(ClientWsMessage::Stop(end_sub)) => {
          let msg = MsgSubscriptionStop {
            sub_id: end_sub.id,
            user_id: self.conn_id.as_ref().unwrap().to_owned(),
          };
          self.tracker.do_send(msg);
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
