use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub name: String,
    pub checksum: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub url: String,
}
