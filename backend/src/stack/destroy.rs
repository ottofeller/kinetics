use crate::auth::session::Session;
use crate::env::env;
use crate::json;
use crate::stack::Stack;
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
        "cloudfront:*",
        "sqs:*",
        "cloudformation:DeleteStack",
        "lambda:RemovePermission",
        "lambda:DeleteEventSourceMapping",
        "lambda:GetEventSourceMapping",
        "lambda:DeleteFunction",
        "lambda:DeleteFunctionUrlConfig",
        "dynamodb:DescribeTable",
        "dynamodb:DeleteTAble",
        "iam:DeleteRolePolicy",
        "iam:DeleteRole"
    ],
    "Resource": "*",
    "Effect": "Allow"
}
*/
#[endpoint(url_path = "/stack/destroy", environment = {
    "TABLE_NAME": "kinetics",
})]
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

    let config = aws_config::defaults(BehaviorVersion::v2025_01_17())
        .load()
        .await;

    let client = aws_sdk_cloudformation::Client::new(&config);
    let stack = Stack::new(&session.username(true), &body.crate_name);

    // It's safe to delete the stack without checking if it exists.
    // CloudFormation will return success even if the stack does not exist.
    client
        .delete_stack()
        .deletion_mode(DeletionMode::Standard)
        .stack_name(stack.name)
        .send()
        .await?;

    Ok(json::response(json!({"message": "Destroyed"}), Some(200))?)
}
