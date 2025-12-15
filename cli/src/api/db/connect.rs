use serde::{Deserialize, Serialize};

/// Response from /stack/db/connect
#[derive(Deserialize, Serialize)]
pub struct Response {
    pub connection_string: String,
}

/// Request body for /stack/db/connect
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub project: String,
}
