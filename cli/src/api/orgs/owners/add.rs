use crate::api::{orgs::validators, request::Validate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub org: String,
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub success: bool,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        let mut errors = Vec::new();

        if !validators::Name::validate(&self.org) {
            errors.push(validators::Name::message());
        }

        if !validators::Email::validate(&self.username) {
            errors.push(validators::Email::message());
        }

        if !errors.is_empty() {
            return Some(errors);
        }

        None
    }
}
