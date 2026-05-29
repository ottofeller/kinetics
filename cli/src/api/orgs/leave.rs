use serde::{Deserialize, Serialize};

use crate::api::{orgs::validators, request::Validate};

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub org: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub success: bool,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        let mut errors = Vec::new();

        // Org name
        if !validators::Name::validate(&self.org) {
            errors.push(validators::Name::message());
        }

        if !errors.is_empty() {
            return Some(errors);
        }

        None
    }
}
