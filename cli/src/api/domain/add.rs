#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub domain: String,
    pub project: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Response {
    /// Domain name (e.g. example.com)
    pub domain: String,

    pub status: DomainStatus,

    /// Nameservers to add to the user domain's DNS settings
    pub nameservers: Vec<String>,
}

/// Domain status.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
pub enum DomainStatus {
    /// Waiting for NS propagation
    Pending,
    /// Domain is active and serving traffic
    Ready,
    /// NS propagation timed out
    Error,

    /// Domain is being deleted
    Deleting,
}

impl std::fmt::Display for DomainStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Ready => write!(f, "Ready"),
            Self::Error => write!(f, "Error"),
            Self::Deleting => write!(f, "Deleting"),
        }
    }
}
