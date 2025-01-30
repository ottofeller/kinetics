use eyre::Context;
use lambda_http::{Body, Error, Request, Response};
use serde::de::DeserializeOwned;

pub fn body<T: DeserializeOwned>(event: Request) -> eyre::Result<T> {
    serde_json::from_slice::<T>(event.body().as_ref())
        .wrap_err("Failed to parse request body as JSON")
}

pub fn response<T: serde::Serialize>(body: T) -> Result<Response<Body>, Error> {
    Ok(Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(serde_json::to_string(&body).unwrap().into())?)
}
