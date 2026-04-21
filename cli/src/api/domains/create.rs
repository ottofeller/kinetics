use crate::api::domains::validators;
use crate::api::request::Validate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project_name: String,
    pub domain_name: String,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        let mut errors = Vec::new();

        if self.project_name.trim().is_empty() {
            errors.push("Invalid \"project\". Must not be empty.".into());
        }

        if !validators::Domain::validate(&self.domain_name) {
            errors.push(validators::Domain::message());
        }

        if !errors.is_empty() {
            return Some(errors);
        }

        None
    }
}

/// Status of domain creation process
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Status {
    /// Domain creation is not yet started
    Pending,

    /// Domain creation is in progress
    InProgress,

    /// Domain creation completed successfully
    Provisioned,

    /// Domain creation failed
    Failed,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub status: Status,
}
