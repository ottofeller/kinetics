use crate::api::request::Validate;
use crate::{api::domains::validators, project::Project};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

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

/// Status of domain creation process
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum Status {
    /// Domain creation is not yet started
    Pending,

    /// Domain creation completed successfully
    Provisioned,

    /// Domain creation failed
    Failed,
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Provisioned => write!(f, "Provisioned"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub status: Status,

    /// Nameservers to be added to the domain configuration at registrar
    pub nameservers: Vec<String>,
}
