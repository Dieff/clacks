use log::{error, warn};
use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
  pub jwt_secret: Option<String>,
  pub db_url: Option<String>,
  pub graphql_port: u32,
  pub management_port: u32,
}

impl Default for AppConfig {
  fn default() -> Self {
    AppConfig {
      jwt_secret: None,
      db_url: None,
      graphql_port: 8000,
      management_port: 7999,
    }
  }
}

impl AppConfig {
  fn env(&mut self) {
    self.db_url = env::var("DATABASE_URL").ok();
    if self.db_url.is_none() {
      warn!("Could not read database url from env");
    }
    if let Ok(secret) = env::var("JWT_SECRET") {
      self.jwt_secret = Some(secret.to_owned());
    }
  }
  fn verify(&self) {
    if self.db_url.is_none() {
      panic!("Missing database url");
    }
    if self.jwt_secret.is_none() {
      panic!("No JWT verification secrets found. Set one with the `JWT_SECRET` variable.");
    }
  }

  pub fn new() -> Self {
    let mut config: Self = Default::default();
    config.env();
    config.verify();
    config
  }
}
