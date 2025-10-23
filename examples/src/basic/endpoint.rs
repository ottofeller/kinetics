use http::{Request, Response};
use kinetics::{
    macros::endpoint,
    tools::{
        config::Config as KineticsConfig,
        http::{Body, PathExt},
    },
};
use serde_json::json;
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

/// REST API endpoint which responds with JSON {"success": true}
///
/// Test locally with the following command:
/// kinetics invoke BasicEndpointEndpoint
#[endpoint(url_path = "/endpoint/{name}/{surname}")]
pub async fn endpoint(
    event: Request<Body>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<String>, BoxError> {
    let name = event.path_param("name").unwrap_or("Korben");
    let surname = event.path_param("surname").unwrap_or("Dallas");
    let resp = Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(
            json!({
                "success": true,
                "name": name,
                "surname": surname
            })
            .to_string(),
        )?;

    Ok(resp)
}
