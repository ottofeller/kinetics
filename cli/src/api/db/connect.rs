use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Response {
    pub connection_string: String,
}

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub project: String,
}
