use crate::{function::Function, project::Project};
use kinetics_parser::Params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod envs;

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub is_hotswap: bool,
    pub project: Project,
    pub secrets: HashMap<String, String>,
    pub functions: Vec<FunctionRequest>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FunctionRequest {
    pub is_deploying: bool,
    pub name: String,
    pub params: Params,
    pub environment: HashMap<String, String>,
}

impl From<&Function> for FunctionRequest {
    fn from(f: &Function) -> Self {
        Self {
            name: f.name.clone(),
            is_deploying: f.is_deploying,
            params: f.params.clone(),
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
