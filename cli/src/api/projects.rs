use crate::project::Project;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub projects: Vec<ProjectInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub url: String,
    pub kvdb: Vec<Kvdb>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kvdb {
    pub name: String,
}

impl From<ProjectInfo> for Project {
    fn from(value: ProjectInfo) -> Self {
        Self {
            path: PathBuf::new(),
            name: value.name,
            url: value.url,
            kvdb: value.kvdb,
        }
    }
}
