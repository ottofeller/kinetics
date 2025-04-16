#[derive(serde::Deserialize, serde::Serialize)]
pub struct JsonBody {
    pub crate_name: String,
    pub function_name: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct JsonResponse {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub expiration: String,
}
