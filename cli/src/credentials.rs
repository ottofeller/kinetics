use chrono::{DateTime, Utc};

/// Credentials to be used with API
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Credentials {
    pub(crate) email: String,
    pub(crate) token: String,
    pub(crate) expires_at: DateTime<Utc>,
}
