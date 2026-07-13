use crate::projects::Kvdb;
use serde::{Deserialize, Serialize};

pub mod sqldb;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Project name, used as a prefix for all resources.
    pub name: String,
    /// Project URL, e.g. https://project-name.kinetics.app.
    pub url: String,
    /// KVDBs to be created.
    pub kvdb: Vec<Kvdb>,
    pub observability: Option<Observability>,
    /// Custom domain name for the project.
    pub domain_name: Option<String>,
    /// Optional organization scope.
    pub org: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observability {
    pub dd_api_key: String,
}
