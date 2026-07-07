use crate::project::Project;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project: Project,
    pub function_name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    /// The date and time that the function was last updated
    /// in ISO-8601 format (YYYY-MM-DDThh:mm:ss.sTZD).
    pub last_modified: Option<String>,
}
