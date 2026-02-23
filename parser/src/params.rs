pub mod cron;
pub mod endpoint;
pub mod worker;

pub use cron::Cron;
pub use endpoint::Endpoint;
pub use worker::Worker;

use crate::environment::Environment;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// The role-specific parameters parsed from the kinetics macro attribute.
/// Carries the full configuration data for each function type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Params {
    Endpoint(Endpoint),
    Cron(Cron),
    Worker(Worker),
}

impl Display for Params {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Params::Endpoint(_) => "endpoint",
            Params::Cron(_) => "cron",
            Params::Worker(_) => "worker",
        };

        write!(f, "{}", str)
    }
}

impl Params {
    pub fn name(&self) -> Option<&String> {
        match self {
            Params::Endpoint(params) => params.name.as_ref(),
            Params::Cron(params) => params.name.as_ref(),
            Params::Worker(params) => params.name.as_ref(),
        }
    }

    pub fn environment(&self) -> &Environment {
        match self {
            Params::Endpoint(params) => &params.environment,
            Params::Cron(params) => &params.environment,
            Params::Worker(params) => &params.environment,
        }
    }
}