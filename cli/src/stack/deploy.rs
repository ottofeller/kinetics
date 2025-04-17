use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct BodyCrate {
    // Full Cargo.toml
    pub toml: String,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct BodyFunction {
    pub name: String,

    // Encrypted name of the zip file with the build in S3 bucket
    pub s3key_encrypted: String,

    // Full Cargo.toml
    pub toml: String,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct JsonBody {
    pub crat: BodyCrate,
    pub functions: Vec<BodyFunction>,
    pub secrets: HashMap<String, String>,
}
