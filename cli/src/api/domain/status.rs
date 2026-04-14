#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
pub enum DomainStatus {
    /// Waiting for NS propagation
    Pending,
    /// Domain is verified and ready to use in a new deployment
    Ready,
    /// Domain is deployed and serving traffic
    Deployed,
    /// NS propagation timed out (after 48 hours waiting for propagation)
    Error,
    /// Domain is being deleted
    Deleting,
}

impl std::fmt::Display for DomainStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Ready => write!(f, "Ready"),
            Self::Deployed => write!(f, "Deployed"),
            Self::Error => write!(f, "Error"),
            Self::Deleting => write!(f, "Deleting"),
        }
    }
}

impl DomainStatus {
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn is_deployed(&self) -> bool {
        matches!(self, Self::Deployed)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }

    pub fn is_deleting(&self) -> bool {
        matches!(self, Self::Deleting)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub domain: String,
    pub project: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Response {
    /// Domain name (e.g. example.com)
    pub domain: String,
    /// Domain status (e.g. Pending)
    pub status: DomainStatus,
    /// Last time the domain was checked
    pub last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
}
