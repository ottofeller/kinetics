use crate::api::domain::add::DomainStatus;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub domain: String,
    pub project: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Response {
    /// Domain name (e.g. example.com)
    pub domain_name: String,
    pub status: DomainStatus,
    /// Whether a deploy is needed to complete removal
    pub requires_deploy: bool,
}
