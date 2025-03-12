use crate::auth::session::Session;
use crate::env::env;
use crate::json;
use aws_config::BehaviorVersion;
use aws_sdk_cloudformation::types::DeletionMode;
use eyre::Result;
use kinetics_macro::endpoint;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use std::collections::HashMap;

#[derive(serde::Deserialize, Debug)]
pub struct JsonBody {
    pub crate_name: String,
}

/*
Permissions:
{
    "Action": [
        "cloudformation:DeleteStack",
    ],
    "Resource": "*",
    "Effect": "Allow"
}
*/
#[endpoint(url_path = "/stack/destroy")]
pub async fn destroy(
    event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let session = Session::new(&event, &env("TABLE_NAME")?).await?;

    if !session.is_valid() {
        eprintln!("Not authorized");
        return json::response(json!({"error": "Unauthorized"}), Some(403));
    }

    let body = json::body::<JsonBody>(event)?;

    let config = aws_config::defaults(BehaviorVersion::v2024_03_28())
        .load()
        .await;

    let client = aws_sdk_cloudformation::Client::new(&config);
    let name = format!("{}-{}", session.username(true), body.crate_name);

    match client
        .delete_stack()
        .deletion_mode(DeletionMode::ForceDeleteStack)
        .stack_name(name)
        .send()
        .await
    {
        Ok(_) => Ok(json::response(json!({"message": "Destroyed"}), Some(200))?),
        Err(e) => {
            eprintln!("Error deleting stack: {}", e);
            json::response(json!({"error": "Failed to destroy stack"}), Some(500))
        }
    }
}
