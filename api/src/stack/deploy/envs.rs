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
        let errors: Vec<String> = self
            .functions
            .iter()
            .filter_map(|(name, environment)| {
                super::validate_user_environment(name, environment).err()
            })
            .collect();

        if errors.is_empty() {
            None
        } else {
            Some(errors)
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub fails: Vec<String>,
}
