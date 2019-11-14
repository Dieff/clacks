use biscuit::errors::{Error as JwtErr, ValidationError};
use biscuit::jwa::*;
use biscuit::*;
use chrono::{Duration as CDuration, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::Duration;

// JWTs will expire in 10 days
const TIME_TO_EXPIRATION: Duration = Duration::from_secs(60 * 60 * 24 * 10);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct JWTClaims {
  name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UserClaims {
  pub name: String,
  pub id: String,
}

pub fn encode_jwt(user_id: &str, user_name: &str, secret: &str) -> String {
  let cur_time: Timestamp = From::from(Utc::now());
  let exp_time: Timestamp = From::from(
    cur_time
      .checked_add_signed(CDuration::from_std(TIME_TO_EXPIRATION).unwrap())
      .unwrap(),
  );
  let signing_secret = jws::Secret::Bytes(secret.as_bytes().to_owned());
  let header = jws::RegisteredHeader {
    algorithm: SignatureAlgorithm::HS256,
    ..Default::default()
  };
  let claims = ClaimsSet::<JWTClaims> {
    registered: RegisteredClaims {
      issuer: Some(FromStr::from_str("MY URL").unwrap()),
      subject: Some(FromStr::from_str(user_id).unwrap()),
      not_before: Some(cur_time),
      expiry: Some(exp_time),
      ..Default::default()
    },
    private: JWTClaims {
      name: user_name.to_owned(),
    },
  };

  let jwt = JWT::new_decoded(From::from(header), claims);
  let token = jwt
    .encode(&signing_secret)
    .unwrap()
    .unwrap_encoded()
    .to_string();
  dbg!(&token);
  token
}

pub fn decode_jwt(jwt: &str, secret: &str) -> Result<UserClaims, JwtErr> {
  let signing_secret = jws::Secret::Bytes(secret.as_bytes().to_owned());
  let token: JWT<JWTClaims, Empty> = JWT::new_encoded(jwt);
  let jwt_data = token
    .into_decoded(&signing_secret, SignatureAlgorithm::HS256)?
    .payload()?
    .to_owned();
  jwt_data.registered.validate(ValidationOptions {
    ..Default::default()
  })?;

  let sub = match jwt_data.registered.subject.ok_or(JwtErr::ValidationError(
    ValidationError::MissingRequiredClaims(vec!["subject".to_owned()]),
  ))? {
    StringOrUri::String(s) => Ok(s),
    _ => Err(JwtErr::GenericError(
      "Could not decode the token subject".to_owned(),
    )),
  }?;
  let name = jwt_data.private.name;
  Ok(UserClaims { name, id: sub })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn jwt_ser_and_deser() {
    let token = encode_jwt("user1", "joe", "12345");
    let invalid_token = encode_jwt("user1", "joe", "BAD SECRET");
    let garbage_token = "asdfasdfasdfasdf".to_owned();
    assert!(decode_jwt(&invalid_token, "12345").is_err());
    assert!(decode_jwt(&garbage_token, "12345").is_err());
    assert_eq!(
      decode_jwt(&token, "12345").unwrap(),
      UserClaims {
        name: "joe".to_owned(),
        id: "user1".to_owned()
      }
    );
  }
}
