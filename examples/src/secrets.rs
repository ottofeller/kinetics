use http::{Request, Response, StatusCode};
use kinetics::macros::endpoint;
use serde_json::json;
use std::collections::HashMap;
use tower::BoxError;

/// Print out a secret value
///
/// The secret must be defined in .env.secrets, in the root of the project (same level as Cargo.toml).
/// Test locally with the following command:
/// kinetics invoke SecretsSecretsUndrscrendpoint
#[endpoint(url_path = "/secrets")]
pub async fn secrets_endpoint(
    _event: Request<()>,
    secrets: &HashMap<String, String>,
) -> Result<Response<String>, BoxError> {
    println!(
        "Found a secret: {}",
        secrets
            .get("SECRET_API_KEY")
            .unwrap_or(&String::from("Not found"))
    );

    let resp = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html")
        .body(json!({"success": true}).to_string())
        .map_err(Box::new)?;

    Ok(resp)
}
