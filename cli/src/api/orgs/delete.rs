use serde::{Deserialize, Serialize};

use crate::api::{orgs::validators, request::Validate};

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        let mut errors = Vec::new();

        // Name
        if !validators::Name::validate(&self.name) {
            errors.push(validators::Name::message());
        }

        if !errors.is_empty() {
            return Some(errors);
        }

        None
    }
}