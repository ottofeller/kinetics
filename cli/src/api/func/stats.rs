use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project_name: String,
    pub function_name: String,
    pub period: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub runs: Runs,
    pub queue: Option<Queue>,
}

/// General stats about function runs
#[derive(Debug, Deserialize, Serialize)]
pub struct Runs {
    pub success: u64,
    pub error: u64,
    pub total: u64,
}

/// Worker specific queue stats
#[derive(Debug, Deserialize, Serialize)]
pub struct Queue {
    /// Messages waiting to be picked up
    pub waiting: u64,

    pub oldest: f64,
    pub in_flight: u64,
    pub completed: u64,
    pub retries: u64,
    pub failed: u64,
}
