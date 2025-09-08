use crate::basic::worker::worker;
use http::{Request, Response, StatusCode};
use kinetics::macros::endpoint;
use kinetics::tools::queue::Client as QueueClient;
use serde_json::json;
use std::collections::HashMap;
use tower::BoxError;

/// Send a message to the queue
#[endpoint(url_path = "/queue")]
pub async fn queue(
    _event: Request<Vec<u8>>,
    _secrets: &HashMap<String, String>,
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
