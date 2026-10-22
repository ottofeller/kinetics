use crate::project::Project;
use crate::request::Validate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project: Project,
    pub function_name: String,
    pub period: Option<String>,
    /// Timestamp to start from. Use for pagination requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<i64>,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        let mut errors = super::validators::validate_names(&self.project, &[&self.function_name])
            .unwrap_or_default();
        if self.cursor.is_some_and(|cursor| cursor < 0) {
            errors.push("Invalid \"cursor\". Must not be negative.".into());
        }
        if errors.is_empty() {
            None
        } else {
            Some(errors)
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub events: Vec<Event>,
    pub period: String,
    /// Timestamp to start from in further pagination requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Event {
    pub timestamp: i64,
    pub message: String,
}
