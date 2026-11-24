use crate::{request::Validate, validators::Email};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub email: String,
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
    pub success: bool,
}
