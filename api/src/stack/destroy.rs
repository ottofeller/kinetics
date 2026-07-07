use crate::project::Project;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub project: Project,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub message: String,
}
