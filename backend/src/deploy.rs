use crate::crat::Crate;
use crate::function::Function;
use crate::json;
use crate::secret::Secret;
use crate::template::Template;
use crate::{auth::session::Session, env::env};
use eyre::Context;
use kinetics_macro::endpoint;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use std::collections::HashMap;

// The request/response payload types are used in CLI crate
#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct BodyCrate {
    // Full Cargo.toml
    pub toml: String,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct BodyFunction {
    pub name: String,

    // Encrypted name of the zip file with the build in S3 bucket
    pub s3key_encrypted: String,

    // Full Cargo.toml
    pub toml: String,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct JsonBody {
    pub crat: BodyCrate,
    pub functions: Vec<BodyFunction>,
    pub secrets: HashMap<String, String>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub enum JsonResponseStatus {
    Failure,
    Success,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct JsonResponse {
    pub message: Option<String>,
    pub status: JsonResponseStatus,
}

/*
Permissions:
{
    "Action": [
        "cloudformation:CreateStack",
        "cloudformation:DeleteStack",
        "cloudformation:UpdateStack",
        "cloudformation:DescribeStacks",
        "iam:DeleteRolePolicy",
        "iam:CreateRole",
        "iam:PutRolePolicy",
        "iam:GetRole",
        "iam:CreateRole",
        "iam:DeleteRole",
        "iam:PassRole",
        "lambda:GetFunction",
        "lambda:DeleteFunction",
        "lambda:CreateFunction",
        "lambda:CreateFunctionUrlConfig",
        "lambda:DeleteFunctionUrlConfig",
        "lambda:AddPermission",
        "lambda:RemovePermission",
        "lambda:GetFunctionUrlConfig",
        "lambda:UpdateFunctionCode",
        "lambda:UpdateFunctionConfiguration",
        "lambda:ListTags",
        "s3:GetObject",
        "cloudfront:*",
        "dynamodb:DescribeTable",
        "dynamodb:CreateTable",
        "dynamodb:DeleteTAble",
        "ssm:GetParameters",
        "ssm:PutParameter",
        "ssm:AddTagsToResource",
        "acm:RequestCertificate",
        "acm:DeleteCertificate",
        "logs:CreateLogGroup",
        "logs:CreateLogStream",
        "logs:PutLogEvents",
    ],
    "Resource": "*",
    "Effect": "Allow"
}
*/
#[endpoint(url_path = "/deploy", environment = {
    "BUCKET_NAME": "kinetics-rust-builds",
    "TABLE_NAME": "kinetics",
    "DANGER_DISABLE_AUTH": "false",
    "S3_KEY_ENCRYPTION_KEY": "fjskoapgpsijtzp",
    "BUILDS_BUCKET": "kinetics-rust-builds"
})]
pub async fn deploy(
    event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let session = Session::new(&event, &env("TABLE_NAME")?).await;

    if env("DANGER_DISABLE_AUTH")? == "false" && !session.as_ref().unwrap().is_valid() {
        eprintln!("Not authorized");
        return json::response(json!({"error": "Unauthorized"}), None);
    }

    let body = json::body::<JsonBody>(event)?;
    let crat = Crate::new(body.crat.toml.clone()).wrap_err("Invalid crate toml")?;
    let session = session.unwrap();

    let secrets = body
        .secrets
        .iter()
        .map(|(k, v)| Secret::new(k, v, &crat, &session.username(true)))
        .collect::<Vec<Secret>>();

    let template = Template::new(
        &crat,
        body.functions
            .iter()
            .map(|f| {
                Function::new(
                    &f.toml,
                    &crat,
                    &f.s3key_encrypted,
                    &env("S3_KEY_ENCRYPTION_KEY").unwrap(),
                    true,
                )
                .unwrap()
            })
            .collect::<Vec<Function>>(),
        secrets.clone(),
        &env("BUILDS_BUCKET")?,
        &session.username(true),
        &session.username(false),
    )
    .await?;

    for secret in secrets.iter() {
        secret.sync().await?;
    }

    template
        .provision()
        .await
        .wrap_err("Failed to provision template")?;

    json::response(
        JsonResponse {
            message: None,
            status: JsonResponseStatus::Success,
        },
        None,
    )
}
