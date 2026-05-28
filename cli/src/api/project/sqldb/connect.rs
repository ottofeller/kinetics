use serde::{Deserialize, Serialize};
use crate::project::Project;

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub connection_string: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub project: Project,
}
