use http::{Method, Request, Response};
use kinetics::tools::config::Config as KineticsConfig;
use kinetics::{macros::endpoint, tools::http::Body};
use serde_json::json;
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

/// REST API endpoint which responds with JSON {"success": true}
///
/// Test locally with the following command:
/// kinetics invoke BasicEndpointEndpoint
#[endpoint(url_path = "/endpoint", methods = ["GET", "POST"])]
pub async fn endpoint(
    event: Request<Body>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<String>, BoxError> {
    match *event.method() {
        Method::POST => {
            let body = event.body();
            println!("Received POST request with body: {:?}", body);
        }
        Method::GET => {
            println!("Received GET request");
        }
        _ => {}
    }

    let resp = Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(json!({"success": true}).to_string())?;

    Ok(resp)
}
