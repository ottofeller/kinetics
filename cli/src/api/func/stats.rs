use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project_name: String,
    pub function_name: String,
    /// The period (measured in days) to get statistics for
    pub period: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub runs: Runs,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Runs {
    pub success: u64,
    pub error: u64,
    pub total: u64,
}
