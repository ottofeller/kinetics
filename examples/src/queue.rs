use crate::basic::worker::worker;
use http::{Request, Response, StatusCode};
use kinetics_lib::macros::endpoint;
use kinetics_lib::tools::config::Config as KineticsConfig;
use kinetics_lib::tools::queue::Client as QueueClient;
use serde_json::json;
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

/// Send a message to the queue
///
/// Test locally with the following command:
/// kinetics invoke QueueQueue --with-queue
#[endpoint(url_path = "/queue")]
pub async fn queue(
    _event: Request<Vec<u8>>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<String>, BoxError> {
    let client = QueueClient::from_worker(worker).await?;
    client.send("Test message").await?;

    let resp = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html")
        .body(json!({"success": true}).to_string())
        .map_err(Box::new)?;
    Ok(resp)
}
