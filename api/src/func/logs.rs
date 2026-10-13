use crate::project::Project;
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
