use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Op {
    Deploy,
    Rollback,
    Destroy,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    pub name: String,
    pub operation: Op,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub status: String,
    pub errors: Option<Vec<String>>,
}
