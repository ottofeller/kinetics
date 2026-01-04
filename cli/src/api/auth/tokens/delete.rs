use crate::api::auth::tokens::validators;
use crate::api::request::Validate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub name: String,
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

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub success: bool,
}
