use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub period: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub email: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}
