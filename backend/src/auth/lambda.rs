use crate::env::env;
use crate::{auth::session::Session, template::Function};
use aws_config::BehaviorVersion;
use aws_sdk_lambda::Client as LambdaClient;
use aws_sdk_sts::Client as StsClient;
use eyre::Context;
use kinetics_macro::endpoint;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct JsonBody {
    pub crate_name: String,
    pub function_name: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct JsonResponse {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub expiration: String,
}

/// Issue temporary AWS credentials for a Lambda function role
///
/// Issue temporary AWS credentials that allow access to resources
/// on behalf of the function.
#[endpoint(url_path = "/auth/lambda", environment = {
    "CREDENTIALS_DURATION_SECONDS": "3600",
    "TABLE_NAME": "kinetics",
    "DANGER_DISABLE_AUTH": "false"
})]
pub async fn lambda(
    event: Request,
    _secrets: &HashMap<String, String>,
    _queues: &HashMap<
        String,
        aws_sdk_sqs::operation::send_message::builders::SendMessageFluentBuilder,
    >,
) -> Result<Response<Body>, Error> {
    let session = Session::new(&event, &env("TABLE_NAME")?)
        .await
        .wrap_err("Failed to get user session")?;

    if env("DANGER_DISABLE_AUTH")? == "false" && !session.is_valid() {
        eprintln!("Not authorized");
        return crate::json::response(json!({"error": "Unauthorized"}), Some(401));
    }

    let body = crate::json::body::<JsonBody>(event).wrap_err("The input is invalid")?;
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let lambda_client = LambdaClient::new(&config);

    let function_config = lambda_client
        .get_function()
        .function_name(Function::full_name(
            &session.username(true),
            &body.crate_name,
            &body.function_name,
        ))
        .send()
        .await
        .wrap_err(format!("Failed to get lambda: {}", body.function_name))?;

    // Extract the IAM role ARN from the function configuration
    let role_arn = function_config
        .configuration()
        .and_then(|config| config.role())
        .ok_or_else(|| eyre::eyre!("Failed to extract role ARN from Lambda function"))?;

    // Assume the role to get temporary credentials
    let sts_client = StsClient::new(&config);
    let credentials_duration = env("CREDENTIALS_DURATION_SECONDS")?
        .parse::<i32>()
        .wrap_err("Failed to parse credentials duration")?;

    // Create a session name based on the function name to identify the session
    let session_name = format!("kinetics-lambda-{}", body.function_name);

    let assume_role_response = sts_client
        .assume_role()
        .role_arn(role_arn)
        .role_session_name(session_name)
        .duration_seconds(credentials_duration)
        .send()
        .await
        .wrap_err("Failed to assume role")?;

    // Extract credentials from the response
    let credentials = assume_role_response
        .credentials()
        .ok_or_else(|| eyre::eyre!("No credentials returned"))?;

    crate::json::response(
        JsonResponse {
            access_key_id: credentials.access_key_id().to_string(),
            secret_access_key: credentials.secret_access_key().to_string(),
            session_token: credentials.session_token().to_string(),
            expiration: credentials.expiration().to_string(),
        },
        None,
    )
}
