use chrono::NaiveDateTime;
use diesel::mysql::MysqlConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};

use crate::schema::*;

pub type DbPool = Pool<ConnectionManager<MysqlConnection>>;

#[derive(Queryable, PartialEq, Debug)]
pub struct DbChannel {
  pub id: i32,
  pub display_name: Option<String>,
  pub created_at: NaiveDateTime,
  pub updated_at: NaiveDateTime,
}

#[derive(Queryable, PartialEq, Debug)]
pub struct DbMessage {
  pub id: i32,
  pub sender: String,
  pub updated_at: NaiveDateTime,
  pub created_at: NaiveDateTime,
  edited: Option<bool>,
  channel_id: i32,
  content: Option<String>,
}

#[derive(Queryable, PartialEq, Debug)]
pub struct DbMessageView {
  id: i32,
  message_id: i32,
  user: String,
  created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[table_name = "channels"]
pub struct NewChannel<'a> {
  pub display_name: &'a str,
}

#[derive(Insertable)]
#[table_name = "messages"]
pub struct NewMessage<'a> {
  pub sender: &'a str,
  pub channel_id: i32,
  pub content: Option<&'a str>,
}

pub fn create_channel(conn: &MysqlConnection, display_name: &str) -> QueryResult<DbChannel> {
  let new_channel = NewChannel { display_name };

  diesel::insert_into(channels::table)
    .values(&new_channel)
    .execute(conn)?;

  Ok(channels::table.order(channels::id.desc()).first(conn)?)
}

pub fn create_message(
  conn: &MysqlConnection,
  sender: &str,
  channel_id: i32,
  content: &str,
) -> QueryResult<DbMessage> {
  let new_message = NewMessage {
    sender,
    channel_id,
    content: Some(content),
  };

  diesel::insert_into(messages::table)
    .values(&new_message)
    .execute(conn)?;

  Ok(messages::table.order(messages::id.desc()).first(conn)?)
}