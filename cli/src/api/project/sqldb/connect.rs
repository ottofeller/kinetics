use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub connection_string: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub project: String,
}
