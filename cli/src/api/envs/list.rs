use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request body for /envs/list
#[derive(Deserialize, Serialize)]
pub struct EnvsListRequest {
    pub project_name: String,
    pub functions_names: Vec<String>,
}

/// Response from /envs/list
pub type EnvsListResponse = HashMap<String, HashMap<String, String>>;
