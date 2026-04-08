use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {}

#[derive(Debug, Serialize, Deserialize)]
pub struct Member {
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Org {
    pub id: String,
    pub name: String,
    pub is_owner: bool,
    pub members: Vec<Member>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub orgs: Vec<Org>,
}
