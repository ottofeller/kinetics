use crate::{project::Project, request::Validate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project: Project,
    pub functions: HashMap<String, HashMap<String, String>>,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        let mut errors: Vec<String> = self
            .functions
            .iter()
            .filter_map(|(name, environment)| {
                super::validate_user_environment(name, environment).err()
            })
            .collect();

        if self.project.name.trim().is_empty() {
            errors.push("Invalid \"project\". Must not be empty.".into());
        }
        if self.functions.keys().any(|name| name.trim().is_empty()) {
            errors.push("Invalid \"functions\". Function names must not be empty.".into());
        }

        if errors.is_empty() {
            None
        } else {
            Some(errors)
        }
    }
}

pub type Response = crate::envs::Response;
