use serde::{Deserialize, Serialize};

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
