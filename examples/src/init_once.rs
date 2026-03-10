use http::{Request, Response};
use kinetics::tools::config::Config as KineticsConfig;
use kinetics::{macros::endpoint, tools::http::Body};
use std::collections::HashMap;
use tokio::sync::OnceCell;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

struct FnConfig {
    pub status: &'static str,
}

static INIT_CELL: OnceCell<&FnConfig> = OnceCell::const_new();
async fn initialize() -> Result<&'static FnConfig, BoxError> {
    println!("Initialized");
    Ok(&FnConfig { status: "Running" })
}

/// REST API endpoint which initializes a string slice "Running"
/// and responds with it.
///
/// Test locally with the following command:
/// kinetics invoke InitInit
#[endpoint(url_path = "/init-once")]
pub async fn init_once(
    _event: Request<Body>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<String>, BoxError> {
    let config = INIT_CELL.get_or_try_init(initialize).await?;
    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/plain")
        .body(format!("Status: {}", config.status))?;

    Ok(resp)
}
