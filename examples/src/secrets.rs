use http::{Request, Response, StatusCode};
use kinetics::tools::config::Config as KineticsConfig;
use kinetics::{macros::endpoint, tools::http::Body};
use serde_json::json;
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

/// Print out a secret value
///
/// The secret must be defined in .env.secrets, in the root of the project (same level as Cargo.toml).
///
/// Test locally with the following command:
/// kinetics invoke SecretsSecrets
#[endpoint(url_path = "/secrets")]
pub async fn secrets(
    _event: Request<Body>,
    secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
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
