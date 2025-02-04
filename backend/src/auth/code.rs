use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use skymacro::endpoint;
use std::collections::HashMap;

#[endpoint(url_path = "/auth/code/request")]
pub async fn request(
    _event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    crate::json::response(json!({}))
}

#[endpoint(url_path = "/auth/code/exchange")]
pub async fn exchange(
    _event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    crate::json::response(json!({"token": "token", "expiresAt": "2020-01-01T01:01:01Z"}))
}
