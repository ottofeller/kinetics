use crate::api::auth::tokens::validators;
use crate::api::request::Validate;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub period: Option<String>,
    pub name: String,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        let mut errors = Vec::new();

        // Period
        if let Some(period) = &self.period {
            if humantime::parse_duration(period).is_err() {
                errors.push(
                    "Invalid \"period\". Expected a duration like '1h', '30m', '7d', etc.".into(),
                );
            }
        }

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
    pub email: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}
