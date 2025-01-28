use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::Client;
use eyre::{Context, ContextCompat, OptionExt};
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use skymacro::endpoint;
use std::collections::HashMap;

fn main() {}

/// Generate S3 presigned URL for upload
#[endpoint(
    name = "upload",
    url_path = "/upload",
    environment = {
        "EXPIRES_IN_SECONDS": "15",
        "BUCKET_NAME": "kinetics-rust-builds"
    },
)]
pub async fn upload(
    event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = Client::new(&config);
    let env = std::env::vars().collect::<HashMap<_, _>>();

    let expires_in: std::time::Duration = std::time::Duration::from_secs(
        env.get("EXPIRES_IN_SECONDS")
            .ok_or_eyre("EXPIRES_IN_SECONDS is missing")?
            .parse()
            .wrap_err("Wrong format of EXPIRES_IN_SECONDS")?,
    );

    let expires_in: PresigningConfig =
        PresigningConfig::expires_in(expires_in).wrap_err("Failed to prepare duration")?;

    let presigned_request = client
        .put_object()
        .bucket(env.get("BUCKET_NAME").wrap_err("BUCKET_NAME is missing")?)
        .key({
            let body: serde_json::Value = serde_json::from_slice(event.body().as_ref())
                .wrap_err("Failed to parse request body as JSON")?;

            body.get("key")
                .wrap_err("No 'key' field found in request body")?
                .as_str()
                .wrap_err("'key' field is not a string")?
                .to_string()
        })
        .presigned(expires_in)
        .await?;

    Ok(Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(json!({"url":  presigned_request.uri()}).to_string().into())
        .map_err(Box::new)?)
}
