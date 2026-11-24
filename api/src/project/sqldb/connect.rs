use crate::project::Project;
use crate::request::Validate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub connection_string: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub project: Project,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        if self.project.name.trim().is_empty() {
            Some(vec!["Invalid \"project\". Must not be empty.".into()])
        } else {
            None
        }
    }
}
