use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project_name: String,
    pub functions: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub fails: Vec<String>,
}
