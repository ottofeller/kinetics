use crate::{project::Project, request::Validate};
use kinetics_parser::{Params, Role, Worker};
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

        if self.project.name.trim().is_empty() {
            errors.push("Invalid \"project\". Must not be empty.".into());
        }

        if self.functions.is_empty() {
            errors.push("Deploy request must include at least one function.".into());
        }

        for function in &self.functions {
            errors.extend(validate_function(function));
        }

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

fn validate_function(function: &FunctionRequest) -> Vec<String> {
    let mut errors = Vec::new();

    if function.name.trim().is_empty() {
        errors.push("Invalid function name. Must not be empty.".into());
    }

    if function.name.chars().count() > 64 {
        errors.push(format!(
            "Invalid function \"{}\". Name must be at most 64 characters.",
            function.name
        ));
    }

    errors.extend(match &function.params {
        Params::Endpoint(endpoint) => validate_endpoint(function, endpoint),
        Params::Cron(cron) => validate_cron(function, cron),
        Params::Worker(worker) => validate_worker(function, worker),
    });

    errors
}

fn validate_endpoint(
    function: &FunctionRequest,
    endpoint: &kinetics_parser::Endpoint,
) -> Vec<String> {
    let mut errors = Vec::new();

    if endpoint.url_path.trim().is_empty() {
        errors.push(format!(
            "Invalid endpoint \"{}\". URL path must not be empty.",
            function.name
        ));
    }

    if !endpoint.url_path.starts_with('/') {
        errors.push(format!(
            "Invalid endpoint \"{}\". URL path must start with '/'.",
            function.name
        ));
    }

    errors
}

fn validate_cron(function: &FunctionRequest, cron: &kinetics_parser::Cron) -> Vec<String> {
    let mut errors = Vec::new();
    let schedule = &cron.schedule;

    // TODO: validate the cron expression itself, not just presence
    if schedule.trim().is_empty() {
        errors.push(format!(
            "Invalid cron \"{}\". Schedule must not be empty.",
            function.name
        ));
    }

    errors
}

fn validate_worker(function: &FunctionRequest, worker: &Worker) -> Vec<String> {
    let mut errors = Vec::new();

    if worker.concurrency < 2 {
        errors.push(format!(
            "Invalid worker \"{}\". Queue concurrency must be at least 2.",
            function.name
        ));
    }

    let max_batch_size = if worker.fifo { 10 } else { 100 };
    if let Some(batch_size) = worker.batch_size {
        if !(1..=max_batch_size).contains(&batch_size) {
            errors.push(format!(
                "Invalid worker \"{}\". Batch size must be 1..{} for {} queues.",
                function.name,
                max_batch_size,
                if worker.fifo { "FIFO" } else { "standard" }
            ));
        }
    }

    errors
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FunctionRequest {
    pub is_deploying: bool,
    pub name: String,
    pub role: Role,
    pub params: Params,
    pub environment: HashMap<String, String>,
    pub secrets: Option<HashMap<String, String>>,
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
