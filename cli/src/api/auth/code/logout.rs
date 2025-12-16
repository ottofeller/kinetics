use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub email: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub success: bool,
}
