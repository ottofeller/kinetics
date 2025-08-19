use kinetics::macros::endpoint;
use kinetics::tools::queue::Client as QueueClient;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use std::collections::HashMap;

/// REST API endpoint which responds with JSON {"success": true}
///
/// Test locally with the following command:
/// kinetics invoke BasicEndpointEndpoint
#[endpoint(url_path = "/endpoint")]
pub async fn endpoint(
    _event: Request,
    _secrets: &HashMap<String, String>,
    _queues: &HashMap<String, QueueClient>,
) -> Result<Response<Body>, Error> {
    let resp = Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(json!({"success": true}).to_string().into())
        .map_err(Box::new)?;

    Ok(resp)
}
