use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Serialize)]
pub struct Request {
    pub project_name: String,
    pub functions_names: Vec<String>,
}

pub type Response = HashMap<String, HashMap<String, String>>;
