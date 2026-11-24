use crate::project::Project;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub name: String,
    pub project: Project,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub status: String,
    pub errors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_details: Option<Vec<crate::error::Error>>,
}
