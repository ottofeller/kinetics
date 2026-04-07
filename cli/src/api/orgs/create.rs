use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
}
