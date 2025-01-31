use crate::crat::Crate;
use crate::function::Function;
use crate::json;
use crate::secret::Secret;
use crate::template::Template;
use aws_config::BehaviorVersion;
use eyre::Context;
use lambda_http::{Body, Error, Request, Response};
use skymacro::endpoint;
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

    // The name of the zip file with the build in S3 bucket
    pub s3key: String,

    // Full Cargo.toml
    pub toml: String,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct JsonBody {
    pub crat: BodyCrate,
    pub functions: Vec<BodyFunction>,
    pub secrets: Vec<HashMap<String, String>>,
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

/// Check if the stack already exists
async fn is_exists(client: &aws_sdk_cloudformation::Client, name: &str) -> eyre::Result<bool> {
    let result = client
        .describe_stacks()
        .set_stack_name(Some(name.into()))
        .send()
        .await;

    if let Err(e) = &result {
        if let aws_sdk_cloudformation::error::SdkError::ServiceError(err) = e {
            if err.err().meta().code().unwrap().eq("ValidationError") {
                return Ok(false);
            } else {
                return Err(eyre::eyre!(
                    "Service error while describing stack: {:?}",
                    err
                ));
            }
        } else {
            return Err(eyre::eyre!("Failed to describe stack: {:?}", e));
        }
    }

    Ok(true)
}

/// Provision cloud resources using CFN template
async fn provision(template: &str, crat: &Crate) -> eyre::Result<()> {
    let config = aws_config::defaults(BehaviorVersion::v2024_03_28())
        .load()
        .await;

    let client = aws_sdk_cloudformation::Client::new(&config);
    let default_name = crat.name.as_str();
    let default_value = toml::Value::String(default_name.to_string());
    let stack = crat.metadata()?;
    let stack = stack.get("stack");

    let name = if stack.is_none() {
        default_name
    } else {
        stack
            .unwrap()
            .get("name")
            .unwrap_or(&default_value)
            .as_str()
            .unwrap()
    };

    let capabilities = aws_sdk_cloudformation::types::Capability::CapabilityIam;

    if is_exists(&client, name).await? {
        client
            .update_stack()
            .capabilities(capabilities)
            .stack_name(name)
            .template_body(template)
            .send()
            .await
            .wrap_err("Failed to update stack")?;
    } else {
        client
            .create_stack()
            .capabilities(capabilities)
            .stack_name(name)
            .template_body(template)
            .send()
            .await
            .wrap_err("Failed to create stack")?;
    }

    Ok(())
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
        "s3:GetObject",
        "cloudfront:*"
    ],
    "Resource": "*",
    "Effect": "Allow"
}
*/
#[endpoint(url_path = "/deploy", environment = {
    "BUCKET_NAME": "kinetics-rust-builds"
})]
pub async fn deploy(
    event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let body = json::body::<JsonBody>(event)?;
    let crat = Crate::new(body.crat.toml.clone()).wrap_err("Invalid crate toml")?;

    let template = Template::new(
        &crat,
        body.functions
            .iter()
            .map(|f| Function::new(&f.toml, &crat, &f.s3key).unwrap())
            .collect::<Vec<Function>>(),
        body.secrets
            .iter()
            .flat_map(|m| m.iter())
            .map(|(k, v)| Secret::new(k, v, &crat, "nide"))
            .collect::<Vec<Secret>>(),
        "kinetics-rust-builds",
        "nide",
    )?;

    provision(&template.to_string(), &crat)
        .await
        .wrap_err("Failed to provision template")?;

    json::response(JsonResponse {
        message: None,
        status: JsonResponseStatus::Success,
    })
}
