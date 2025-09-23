use http::{Request, Response};
use kinetics::macros::endpoint;
use kinetics::tools::config::Config as KineticsConfig;
use serde_json::json;
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

/// REST API endpoint which responds with a value of environment variable
///
/// Test locally with the following command:
/// kinetics invoke EnvironmentEnvironment
#[endpoint(
    url_path = "/environment",
    environment = {"SOME_VAR": "someval"},
)]
pub async fn environment(
    _event: Request<String>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<String>, BoxError> {
    let env = std::env::vars().collect::<HashMap<_, _>>();

    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body(
            json!({"SOME_VAR": env.get("SOME_VAR").unwrap_or(&String::from("Not set"))})
                .to_string(),
        )?;

    Ok(resp)
}
