use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub project_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub message: String,
}
