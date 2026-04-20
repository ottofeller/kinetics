use crate::api::request::Validate;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// DNS name per RFC 1035: each label is 1–63 chars of `[a-z0-9-]` and cannot start or end with a
/// hyphen, followed by a TLD of 2+ letters.
static DOMAIN_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?i)(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,}$")
        .expect("Failed to init regexp")
});

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

        if self.domain_name.len() > 253 || !DOMAIN_REGEX.is_match(&self.domain_name) {
            errors.push("Invalid \"domain\". Must be a valid DNS name (e.g. example.com).".into());
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
