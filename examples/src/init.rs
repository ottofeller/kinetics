use http::{Request, Response};
use kinetics::tools::config::Config as KineticsConfig;
use kinetics::{macros::endpoint, tools::http::Body};
use std::collections::HashMap;
use tokio::sync::OnceCell;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

static INIT_CELL: OnceCell<&str> = OnceCell::const_new();
async fn initialize() -> Result<&'static str, BoxError> {
    println!("Initialized");
    Ok("Running")
}

/// REST API endpoint which initializes a string slice "Running"
/// and responds with it.
///
/// Test locally with the following command:
/// kinetics invoke InitInit
#[endpoint(url_path = "/init")]
pub async fn init(
    _event: Request<Body>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<String>, BoxError> {
    let status = INIT_CELL.get_or_try_init(initialize).await?;
    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/plain")
        .body(format!("Status: {status}"))?;

    Ok(resp)
}
