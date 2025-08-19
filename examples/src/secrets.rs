use kinetics::macros::endpoint;
use kinetics::tools::queue::Client as QueueClient;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use std::collections::HashMap;

/// Print out a secret value
///
/// The secret must be defined in .env.secrets, in the root of the project (same level as Cargo.toml).
/// Test locally with the following command:
/// kinetics invoke SecretsSecretsUndrscrendpoint
#[endpoint(url_path = "/secrets")]
pub async fn secrets_endpoint(
    _event: Request,
    secrets: &HashMap<String, String>,
    _queues: &HashMap<String, QueueClient>,
) -> Result<Response<Body>, Error> {
    println!(
        "Found a secret: {}",
        secrets
            .get("SECRET_API_KEY")
            .unwrap_or(&String::from("Not found"))
    );

    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body(json!({"success": true}).to_string().into())
        .map_err(Box::new)?;

    Ok(resp)
}
