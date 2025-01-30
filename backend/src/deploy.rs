use crate::crat::Crate;
use crate::function::Function;
use crate::json;
use crate::secret::Secret;
use crate::template::Template;
use eyre::Context;
use lambda_http::{Body, Error, Request, Response};
use skymacro::endpoint;
use std::collections::HashMap;

// The request/response payload types are used in CLI crate
#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct BodyCrate {
    // Full Cargo.toml
    pub toml: String,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct BodyFunction {
    pub name: String,

    // The name of the zip file with the build in S3 bucket
    pub s3key: String,

    // Full Cargo.toml
    pub toml: String,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct JsonBody {
    pub crat: BodyCrate,
    pub functions: Vec<BodyFunction>,
    pub secrets: Vec<HashMap<String, String>>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub enum JsonResponseStatus {
    Failure,
    Success,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct JsonResponse {
    pub message: Option<String>,
    pub status: JsonResponseStatus,
}

#[endpoint(url_path = "/deploy", environment = {
    "BUCKET_NAME": "kinetics-rust-builds"
})]
pub async fn deploy(
    event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let body = json::body::<JsonBody>(event)?;
    let crat = Crate::new(body.crat.toml.clone()).wrap_err("Invalid crate toml")?;
    println!("{body:?}");

    let template = Template::new(
        &crat,
        body.functions
            .iter()
            .map(|f| Function::new(&f.toml, &crat, &f.s3key).unwrap())
            .collect::<Vec<Function>>(),
        body.secrets
            .iter()
            .flat_map(|m| m.iter())
            .map(|(k, v)| Secret::new(k, v, &crat, "nide"))
            .collect::<Vec<Secret>>(),
        "kinetics-rust-builds",
        "nide",
    )?;

    println!("{template:?}");

    json::response(JsonResponse {
        message: None,
        status: JsonResponseStatus::Success,
    })
}
