use crate::{request::Validate, validators::Email};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub email: String,
    pub code: String,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        if !Email::validate(&self.email) {
            return Some(vec![Email::message()]);
        }

        None
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub email: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}
