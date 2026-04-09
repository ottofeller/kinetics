pub mod add;
pub mod remove;
pub mod status;

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
