use crate::request::Validate;
use crate::{domains::validators, project::Project};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project: Project,
    pub domain_name: String,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        let mut errors = Vec::new();

        if self.project.name.trim().is_empty() {
            errors.push("Invalid \"project\". Must not be empty.".into());
        }

        if !validators::Name::validate(&self.domain_name) {
            errors.push(validators::Name::message());
        }

        if !errors.is_empty() {
            return Some(errors);
        }

        None
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {}
