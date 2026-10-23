pub mod list;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub type Variables = HashMap<String, String>;

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub results: BTreeMap<String, FunctionResult>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FunctionResult {
    Success {
        #[serde(skip_serializing_if = "Option::is_none")]
        envs: Option<Variables>,
    },
    Error {
        error: crate::error::Error,
    },
}
