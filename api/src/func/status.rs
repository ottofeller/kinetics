use crate::project::Project;
use crate::request::Validate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FunctionName {
    Single(String),
    List(Vec<String>),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub project: Project,
    pub function_name: FunctionName,
}

impl Validate for Request {
    fn validate(&self) -> Option<Vec<String>> {
        match &self.function_name {
            FunctionName::Single(name) => super::validators::validate_names(&self.project, &[name]),
            FunctionName::List(names) => super::validators::validate_names(
                &self.project,
                &names.iter().map(String::as_str).collect::<Vec<_>>(),
            ),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub enum SingleFunctionStatus {
    Modified(String),
    NotModified,
    NotFound,
    Error(String),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FunctionStatus {
    Single(SingleFunctionStatus),
    List(Vec<SingleFunctionStatus>),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    /// The date and time that the function was last updated
    /// in ISO-8601 format (YYYY-MM-DDThh:mm:ss.sTZD).
    pub last_modified: FunctionStatus,
}
