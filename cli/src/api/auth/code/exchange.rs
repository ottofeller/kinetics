use crate::credentials::Credentials;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub email: String,
    pub code: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub email: String,
    pub token: String,
    pub expires_at: String,
}

impl TryFrom<Response> for Credentials {
    type Error = eyre::Report;

    fn try_from(value: Response) -> eyre::Result<Self> {
        Ok(Self {
            path: PathBuf::new(),
            email: value.email,
            token: value.token,
            expires_at: DateTime::parse_from_rfc3339(&value.expires_at)?.to_utc(),
        })
    }
}
