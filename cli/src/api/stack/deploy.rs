use crate::api::request::Validate;
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
