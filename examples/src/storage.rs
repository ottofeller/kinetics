use http::{Request, Response, StatusCode};
use kinetics_lib::macros::endpoint;
use kinetics_lib::tools::config::Config as KineticsConfig;
use kinetics_lib::tools::storage::Client as StorageClient;
use serde_json::json;
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

/// Store a file in the project storage and read it back
#[endpoint(url_path = "/storage")]
pub async fn storage(
    _event: Request<Vec<u8>>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<String>, BoxError> {
    let client = StorageClient::new().await?;

    client
        .upload("greeting.txt", "hello from kinetics".as_bytes().to_vec())
        .await?;
    let contents = client.download("greeting.txt").await?;
    let greeting = String::from_utf8_lossy(&contents);
    let files = client.list().await?;

    let resp = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html")
        .body(
            json!({
                "success": true,
                "greeting": greeting,
                "files": files.iter().map(|file| &file.key).collect::<Vec<_>>(),
            })
            .to_string(),
        )
        .map_err(Box::new)?;
    Ok(resp)
}
