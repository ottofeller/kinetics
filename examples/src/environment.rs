use kinetics::macros::endpoint;
use kinetics::tools::queue::Client as QueueClient;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use std::collections::HashMap;

/// REST API endpoint which responds with a value of environment variable
///
/// Test locally with the following command:
/// kinetics invoke EnvironmentEnvironment
#[endpoint(
    url_path = "/environment",
    environment = {"SOME_VAR": "someval"},
)]
pub async fn environment(
    _event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let env = std::env::vars().collect::<HashMap<_, _>>();

    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body(
            json!({"SOME_VAR": env.get("SOME_VAR").unwrap_or(&String::from("Not set"))})
                .to_string()
                .into(),
        )
        .map_err(Box::new)?;

    Ok(resp)
}
