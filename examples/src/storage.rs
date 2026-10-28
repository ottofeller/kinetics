use http::{Method, Request, Response, StatusCode};
use kinetics_lib::macros::endpoint;
use kinetics_lib::tools::config::Config as KineticsConfig;
use kinetics_lib::tools::storage::Client as StorageClient;
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

/// File kept in the project storage by this example.
const KEY: &str = "greeting.txt";

/// Store the request body (PUT) or read the stored file back (GET).
#[endpoint(url_path = "/storage", methods = ["GET", "PUT"])]
pub async fn storage(
    event: Request<Vec<u8>>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<String>, BoxError> {
    let client = StorageClient::new().await?;

    let (status, body) = match *event.method() {
        Method::PUT => {
            let body = event.into_body();
            client.upload(KEY, body).await?;
            (StatusCode::OK, None)
        }
        Method::GET => {
            if client.exists(KEY).await? {
                let contents = client.download(KEY).await?;
                (
                    StatusCode::OK,
                    Some(String::from_utf8_lossy(&contents).to_string()),
                )
            } else {
                (StatusCode::NOT_FOUND, None)
            }
        }
        _ => (StatusCode::METHOD_NOT_ALLOWED, None),
    };

    let response = Response::builder()
        .status(status)
        .header("content-type", "text/html")
        .body(body.unwrap_or_default())
        .map_err(Box::new)?;

    Ok(response)
}
