use crate::api::request::Validate;
use crate::config::build_config;
use crate::{function::Function, project::Project};
use kinetics_parser::{Params, Role};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod envs;

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub is_hotswap: bool,
    pub project: Project,
    pub secrets: HashMap<String, String>,
    pub functions: Vec<FunctionRequest>,
    pub version_message: Option<String>,
}

const MAX_MESSAGE_LENGTH: usize = 100;

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        let mut errors = Vec::new();

        if let Some(message) = &self.version_message {
            if message.chars().count() > MAX_MESSAGE_LENGTH {
                errors.push(format!(
                    "message must be at most {} characters, got {}",
                    MAX_MESSAGE_LENGTH,
                    message.chars().count()
                ));
            }
        }

        if let Some(observability) = &self.project.observability {
            if observability.dd_api_key.is_empty() {
                errors.push(
                    "DataDog API key is missing in [observability] section of kinetics.toml".into(),
                );
            }
        }

        if let Some(domain_name) = &self.project.domain_name {
            if domain_name.is_empty() {
                errors.push("Domain cannot be empty".into());
            }

            let fqdn_re = regex::Regex::new(
                r"^(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,}$",
            )
            .unwrap();

            if !fqdn_re.is_match(domain_name.trim()) {
                errors.push(format!("Invalid domain format: {}", domain_name.trim()));
            }

            // Check if the domain is different from the service domain
            if let Ok(config) = build_config() {
                let service_domain = config.domain_name;

                if domain_name == service_domain
                    || domain_name.ends_with(&format!(".{service_domain}"))
                {
                    errors.push(format!("Cannot use {service_domain} or its subdomains"));
                }
            } else {
                errors.push("Failed to get build config".into());
            }
        }

        if !errors.is_empty() {
            return Some(errors);
        }

        None
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FunctionRequest {
    pub is_deploying: bool,
    pub name: String,
    pub role: Role,
    pub params: Params,
    pub environment: HashMap<String, String>,
}

impl From<&Function> for FunctionRequest {
    fn from(f: &Function) -> Self {
        Self {
            name: f.name.clone(),
            is_deploying: f.is_deploying,
            params: f.params.clone(),
            role: f.role.clone(),
            environment: f.environment(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub message: Option<String>,
    pub status: ResponseStatus,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ResponseStatus {
    Failure,
    Success,
    NotChanged,
}
