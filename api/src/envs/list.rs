use crate::project::Project;
use crate::request::Validate;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Request {
    pub project: Project,
    pub functions_names: Vec<String>,
    #[serde(default)]
    pub detailed_results: bool,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        let mut errors = Vec::new();
        if self.project.name.trim().is_empty() {
            errors.push("Invalid \"project\". Must not be empty.".into());
        }
        if self
            .functions_names
            .iter()
            .any(|name| name.trim().is_empty())
        {
            errors.push("Invalid \"functions_names\". Function names must not be empty.".into());
        }
        if errors.is_empty() {
            None
        } else {
            Some(errors)
        }
    }
}

pub type Response = super::Response;
