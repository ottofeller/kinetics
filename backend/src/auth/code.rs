use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::types::AttributeValue::S;
use aws_sdk_dynamodb::Client;
use eyre::{Context, ContextCompat};
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use skymacro::endpoint;
use std::collections::HashMap;

/// Generate a tmp auth code for the user to login, and send over email
#[endpoint(url_path = "/auth/code/request", environment = {"TABLE_NAME": "kinetics"})]
pub async fn request(
    event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    #[derive(serde::Deserialize)]
    pub struct JsonBody {
        pub email: serde_email::Email,
    }

    let env = std::env::vars().collect::<HashMap<_, _>>();
    let body = crate::json::body::<JsonBody>(event).wrap_err("The input is invalid")?;
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Client::new(&config);

    client
        .put_item()
        .table_name(env.get("TABLE_NAME").wrap_err("TABLE_NAME is missing")?)
        .set_item(Some(HashMap::from([
            (
                "id".to_string(),
                S(format!("{}#authcode", body.email.to_string())),
            ),
            ("created_at".to_string(), S(chrono::Utc::now().to_rfc3339())),
            (
                "code".to_string(),
                S(format!("{:X}", rand::random::<u32>() % 1000000000).to_lowercase()),
            ),
        ])))
        .send()
        .await?;

    crate::json::response(json!({"success": true}))
}

/// Exchange the auth code for a short lived access token
#[endpoint(url_path = "/auth/code/exchange")]
pub async fn exchange(
    _event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    crate::json::response(json!({"token": "token", "expiresAt": "2020-01-01T01:01:01Z"}))
}
