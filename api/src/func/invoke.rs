use crate::project::Project;
use crate::request::Validate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project: Project,
    pub function_name: String,
    /// Payload is set only for worker invocation.
    pub payload: Option<String>,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        super::validators::validate_names(&self.project, &[&self.function_name])
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub enum Status {
    NotStarted(String),
    Success,
    Fail(String),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub status: Status,
    pub log: Option<String>,
    /// Response or an error.
    pub payload: Option<Vec<u8>>,
}
