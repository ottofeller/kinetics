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
const MAX_USER_ENVIRONMENT_BYTES: usize = 3 * 1024;

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
            if !self.is_hotswap && function.is_deploying {
                if let Err(error) = validate_user_environment(&function.name, &function.environment)
                {
                    errors.push(error);
                }
            }
        }

        errors.extend(validate_dlq_links(&self.functions));

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

/// Check the size of a function's user-supplied Lambda environment map.
fn validate_user_environment(
    function_name: &str,
    environment: &HashMap<String, String>,
) -> Result<(), String> {
    let size = serde_json::to_vec(environment)
        .map_err(|error| {
            format!(
                "Could not serialize user environment for function \"{function_name}\": {error}"
            )
        })?
        .len();

    if size > MAX_USER_ENVIRONMENT_BYTES {
        return Err(format!(
            "User environment size exceeds limit for function \"{}\": {} bytes (max: {} bytes)",
            function_name, size, MAX_USER_ENVIRONMENT_BYTES
        ));
    }

    Ok(())
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
        Params::DlqWorker(dlq) => validate_dlq_worker(function, dlq),
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

fn max_batch_size(fifo: bool) -> u32 {
    if fifo {
        10
    } else {
        100
    }
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

    if let Some(count) = worker.max_receive_count {
        if !(1..=1000).contains(&count) {
            errors.push(format!(
                "Invalid worker \"{}\". `max_receive_count` must be 1..=1000, got {count}.",
                function.name
            ));
        }
    }

    let max_batch_size = max_batch_size(worker.fifo);
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

fn validate_dlq_worker(
    function: &FunctionRequest,
    dlq: &kinetics_parser::DlqWorker,
) -> Vec<String> {
    let mut errors = Vec::new();

    if dlq.concurrency < 2 {
        errors.push(format!(
            "Invalid dlq_worker \"{}\". Queue concurrency must be at least 2.",
            function.name
        ));
    }

    // The batch size bound needs the main worker's FIFO flag
    // and is checked in validate_dlq_links.

    errors
}

/// Verify worker to DLQ worker links and the FIFO-inherited batch bound.
///
/// Upon calling the validator the `dlq` prop
/// already holds full name of the linked `#[dlq_worker]`.
/// Check that the target:
/// - exists in the project,
/// - has the required type,
/// - is linked from a single worker,
/// - conforms to main worker batch size rules.
fn validate_dlq_links(functions: &[FunctionRequest]) -> Vec<String> {
    let mut errors = Vec::new();

    let fifo_dlqs: HashMap<&str, bool> = functions
        .iter()
        .filter_map(|function| {
            if let Params::Worker(Worker {
                dlq: Some(dlq),
                fifo,
                ..
            }) = &function.params
            {
                Some((dlq.as_str(), *fifo))
            } else {
                None
            }
        })
        .collect();

    for function in functions {
        match &function.params {
            // `dlq` param of a worker must resolve to an existing function.
            Params::Worker(worker) => {
                let Some(target) = &worker.dlq else {
                    continue;
                };

                match functions.iter().find(|other| &other.name == target) {
                    None => errors.push(format!(
                        "`dlq = {target}` on \"{}\" does not resolve to any function in the project",
                        function.name
                    )),
                    Some(linked) => {
                        if !matches!(linked.params, Params::DlqWorker(_)) {
                            errors.push(format!(
                                "`dlq = {target}` on \"{}\" must reference a #[dlq_worker] function",
                                function.name
                            ));
                        }
                    }
                }
            }
            Params::DlqWorker(dlq) => {
                // Every DLQ worker must be linked from a worker,
                // and a single worker owns the queue it consumes.
                let referrers: Vec<&str> = functions
                    .iter()
                    .filter_map(|other| match &other.params {
                        Params::Worker(worker)
                            if worker.dlq.as_deref() == Some(function.name.as_str()) =>
                        {
                            Some(other.name.as_str())
                        }
                        _ => None,
                    })
                    .collect();

                if referrers.is_empty() {
                    errors.push(format!(
                        "Invalid dlq_worker \"{}\": not linked from any worker",
                        function.name
                    ));
                } else if referrers.len() > 1 {
                    errors.push(format!(
                        "Invalid dlq_worker \"{}\": referenced by several workers: {}",
                        function.name,
                        referrers.join(", ")
                    ));
                }

                // DQL worker batch size must be within allowed range.
                if let Some(batch_size) = dlq.batch_size {
                    let fifo = fifo_dlqs
                        .get(function.name.as_str())
                        .copied()
                        .unwrap_or(false);
                    let max_batch_size = max_batch_size(fifo);
                    if !(1..=max_batch_size).contains(&batch_size) {
                        errors.push(format!(
                            "Invalid dlq_worker \"{}\". Batch size must be 1..{} for {} queues.",
                            function.name,
                            max_batch_size,
                            if fifo { "FIFO" } else { "standard" }
                        ));
                    }
                }
            }
            _ => {}
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
    /// Secrets applied only to this function
    pub secrets: Option<FunctionSecrets>,
}

/// Function-level secrets scoped by their workspace package
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FunctionSecrets {
    pub scope: String,
    pub values: HashMap<String, String>,
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
