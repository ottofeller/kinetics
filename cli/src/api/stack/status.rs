use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub status: String,
    pub errors: Option<Vec<String>>,
}
