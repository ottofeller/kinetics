use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request body for /envs/list
#[derive(Deserialize, Serialize)]
pub struct Request {
    pub project_name: String,
    pub functions_names: Vec<String>,
}

/// Response from /envs/list
pub type Response = HashMap<String, HashMap<String, String>>;
