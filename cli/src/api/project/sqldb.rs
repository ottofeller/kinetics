use serde::{Deserialize, Serialize};

/// Response from /stack/sqldb/connect
#[derive(Deserialize, Serialize)]
pub struct Response {
    pub connection_string: String,
}

/// Request body for /stack/sqldb/connect
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub project: String,
}
