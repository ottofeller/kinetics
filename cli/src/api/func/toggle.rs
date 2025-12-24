use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::*;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Op {
    Start,
    Stop,
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Op::Start => write!(f, "Starting"),
            Op::Stop => write!(f, "Stopping"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project_name: String,
    pub function_name: String,
    pub operation: Op,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    /// Datetime when throttling was applied
    pub throttled_at: DateTime<Utc>,

    /// The reason for throttling,
    /// e.g. user request or account limit.
    pub reason: String,
}
