use crate::project::Project;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project: Project,
    pub function_name: String,
    pub period: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub events: Vec<Event>,
    pub period: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Event {
    pub timestamp: i64,
    pub message: String,
}
