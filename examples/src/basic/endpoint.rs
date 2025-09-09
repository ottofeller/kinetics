use http::{Request, Response};
use kinetics::{macros::endpoint, tools::http::Body};
use serde_json::json;
use std::collections::HashMap;
use tower::BoxError;

/// REST API endpoint which responds with JSON {"success": true}
///
/// Test locally with the following command:
/// kinetics invoke BasicEndpointEndpoint
#[endpoint(url_path = "/endpoint")]
pub async fn endpoint(
    _event: Request<Body>,
    _secrets: &HashMap<String, String>,
) -> Result<Response<String>, BoxError> {
    let resp = Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(json!({"success": true}).to_string())?;

    Ok(resp)
}
