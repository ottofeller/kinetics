use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Request {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub versions: Vec<Version>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub version: u32,
    pub updated_at: DateTime<Utc>,
}
