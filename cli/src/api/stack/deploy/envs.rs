use crate::project::Project;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project: Project,
    pub functions: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub fails: Vec<String>,
}
