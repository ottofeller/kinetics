#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct JsonBody {
    pub name: String,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct ResponseBody {
    pub status: String,
    pub errors: Option<Vec<String>>,
}
