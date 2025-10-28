use http::{Request, Response};
use kinetics::tools::config::Config as KineticsConfig;
use kinetics::{macros::endpoint, tools::http::Body};
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

#[derive(Debug, Clone, Copy)]
struct InternalError {}

impl std::fmt::Display for InternalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Something went wrong")
    }
}

impl std::error::Error for InternalError {}

/// REST API endpoint handles URL path parameters
///
/// Test locally with the following command:
/// kinetics invoke BasicUrlHello --url-path /hello/john/smith/jr
#[endpoint(url_path = "/hello/{name}/{*rest}")]
pub async fn path(
    event: Request<Body>,
    _secrets: &HashMap<String, String>,
    config: &KineticsConfig,
) -> Result<Response<String>, BoxError> {
    let mut router = matchit::Router::new();
    router.insert(
        config
            .endpoint
            .as_ref()
            .expect("Endpoint must have its config set")
            .url_pattern
            .as_ref()
            .expect("The pattern exists"),
        (),
    )?;

    let matched = router.at(event.uri().path())?;
    let name = matched.params.get("name").ok_or(InternalError {})?;
    let surname = matched.params.get("rest").ok_or(InternalError {})?;
    // Non-existing param
    let response = matched.params.get("response").unwrap_or("Hello");

    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/plain")
        .body(format!("{response}, mr. {name} {surname}"))?;

    Ok(resp)
}
