use crate::gqln::GqlSchema;
use crate::models::DbPool;
use crate::ws_actors::ConnectionTracker;

use actix::Addr;

#[derive(Clone)]
pub struct GqlContext {
  pub cur_user: String,
  pub db: DbPool,
  pub ws_addr: Addr<ConnectionTracker>,
}

impl GqlContext {
  pub fn new(db: DbPool, cur_user: String, ws_addr: Addr<ConnectionTracker>) -> Self {
    Self {
      cur_user,
      db,
      ws_addr,
    }
  }
}

pub type Schema = GqlSchema<GqlContext>;
