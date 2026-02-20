use http::header::CONTENT_TYPE;
use http::{Request, Response, StatusCode};
use kinetics::tools::config::Config as KineticsConfig;
use kinetics::tools::http::Error;
use kinetics::{macros::endpoint, tools::http::Body};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
struct Payload {
    message: String,
}

/// REST API endpoint which responds with received message.
///
/// Test locally with the following command:
/// kinetics invoke ErrorsEcho
#[endpoint(url_path = "/echo")]
pub async fn echo(
    event: Request<Body>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<String>, Error> {
    let body = String::try_from(event.body()).map_err(|e| Error::BadRequest(e.to_string()))?;
    let payload =
        serde_json::from_str::<Payload>(&body).map_err(|e| Error::BadRequest(e.to_string()))?;

    let resp = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/plain")
        .body(payload.message)
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok(resp)
}
