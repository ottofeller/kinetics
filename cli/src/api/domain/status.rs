use crate::api::domain::DomainStatus;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub domain: String,
    pub project: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Response {
    /// Domain name (e.g. example.com)
    pub domain_name: String,
    /// Domain status (e.g. Pending)
    pub status: DomainStatus,
    /// Last time the domain was checked
    pub last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
}
