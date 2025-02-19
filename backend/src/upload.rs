use crate::json;
use crate::{auth::session::Session, env::env};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::Client;
use eyre::Context;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use skymacro::endpoint;
use std::collections::HashMap;

// Permissions:
// {
//     "Action": [
//         "s3:PutObject"
//     ],
//     "Resource": [
//         "*",
//         "arn:aws:s3:::kinetics-rust-builds/*"
//     ],
//     "Effect": "Allow"
// }

/// Generate S3 presigned URL for upload
#[endpoint(
    name = "upload",
    url_path = "/upload",
    environment = {
        "EXPIRES_IN_SECONDS": "15",
        "BUILDS_BUCKET": "kinetics-rust-builds",
        "TABLE_NAME": "kinetics",
        "DANGER_DISABLE_AUTH": "false",
        "S3_KEY_ENCRYPTION_KEY": "fjskoapgpsijtzp"
    },
)]
pub async fn upload(
    event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let session = Session::new(&event, &env("TABLE_NAME")?).await;

    if env("DANGER_DISABLE_AUTH")? == "false" && !session.as_ref().unwrap().is_valid() {
        eprintln!("Not authorized");
        return json::response(json!({"error": "Unauthorized"}), None);
    }

    let session = session.unwrap();
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = Client::new(&config);

    let expires_in: std::time::Duration = std::time::Duration::from_secs(
        env("EXPIRES_IN_SECONDS")?
            .parse()
            .wrap_err("Env var has wrong format (expcted int, seconds)")?,
    );

    let expires_in: PresigningConfig =
        PresigningConfig::expires_in(expires_in).wrap_err("Failed to prepare duration")?;

    let key = format!("{}-{}.zip", session.username(true), uuid::Uuid::new_v4());

    let encrypted_key = {
        use magic_crypt::{new_magic_crypt, MagicCryptTrait};
        let mc = new_magic_crypt!(env("S3_KEY_ENCRYPTION_KEY")?, 256);
        mc.encrypt_str_to_base64(&key)
    };

    let presigned_request = client
        .put_object()
        .bucket(env("BUILDS_BUCKET")?)
        .key(key)
        .presigned(expires_in)
        .await?;

    json::response(
        json!({
            "url":  presigned_request.uri(),
            "s3key_encrypted": encrypted_key,
        }),
        None,
    )
}
