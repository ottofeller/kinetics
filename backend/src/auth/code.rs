use crate::{env::env, user};
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::types::AttributeValue::S;
use aws_sdk_dynamodb::Client;
use eyre::Context;
use kinetics_macro::endpoint;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use std::collections::HashMap;

// Generate a tmp auth code for the user to login, and send over email
#[endpoint(url_path = "/auth/code/request", environment = {"TABLE_NAME": "kinetics", "EXPIRES_IN_SECONDS": "60"})]
pub async fn request(
    event: Request,
    secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    #[derive(serde::Deserialize)]
    struct JsonBody {
        email: serde_email::Email,
    }

    let body = crate::json::body::<JsonBody>(event).wrap_err("The input is invalid")?;
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Client::new(&config);
    let code = format!("{:x}", rand::random::<u32>() % 1000000000);
    let now = chrono::Utc::now();

    client
        .put_item()
        .table_name(env("TABLE_NAME")?)
        .set_item(Some(HashMap::from([
            ("id".to_string(), S(format!("authcode#{}", body.email))),
            ("created_at".to_string(), S(now.to_rfc3339())),
            (
                "expires_at".to_string(),
                S(now
                    .checked_sub_signed(chrono::Duration::seconds(
                        env("EXPIRES_IN_SECONDS")?.parse()?,
                    ))
                    .unwrap()
                    .to_rfc3339()),
            ),
            ("code".to_string(), S(code.clone())),
        ])))
        .send()
        .await?;

    let res = reqwest::Client::new()
        .post("https://api.resend.com/emails")
        .header(
            "Authorization",
            format!("Bearer {}", secrets.get("RESEND_API_KEY").unwrap()),
        )
        .header("Content-Type", "application/json")
        .json(&json!({
            "from": "kinetics@microfeller.com",
            "to": body.email.to_string(),
            "subject": "Auth code",
            "text": format!("Use this code to login using Kinetics CLI:\n{code}")
        }))
        .send()
        .await?;

    if !res.status().is_success() {
        return Err(Error::from(res.text().await.unwrap_or_default()));
    }

    crate::json::response(json!({"success": true}), None)
}

/// Exchange the auth code for a short lived access token
#[endpoint(
    url_path = "/auth/code/exchange",
    environment = {"TABLE_NAME": "kinetics", "ACCESS_TOKEN_EXPIRES_IN_SECONDS": "21600"}
)]
pub async fn exchange(
    event: Request,
    secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    #[derive(serde::Deserialize)]
    struct JsonBody {
        email: serde_email::Email,
        code: String,
    }

    let body = crate::json::body::<JsonBody>(event).wrap_err("The input is invalid")?;
    let client = Client::new(&aws_config::load_defaults(BehaviorVersion::latest()).await);
    let now = chrono::Utc::now();
    let email = body.email.to_string();
    let table = env("TABLE_NAME")?;

    // Mark the auth code as exchanged
    let result = client
        .update_item()
        .table_name(&table)
        .key("id", S(format!("authcode#{}", email)))
        .update_expression("SET exchanged_at = :now")
        .condition_expression(
            "attribute_not_exists(exchanged_at) AND code = :code AND expires_at < :now",
        )
        .expression_attribute_values(":now", S(now.to_rfc3339()))
        .expression_attribute_values(":code", S(body.code))
        .send()
        .await;

    match result {
        Ok(_) => (),
        Err(err) => {
            if err.to_string().contains("ConditionalCheckFailed") {
                return Err(Error::from("Code has already been exchanged"));
            }
            return Err(Error::from(err));
        }
    }

    // Generate and store access token
    let token = sha256::digest(rand::random::<u32>().to_string()).to_string();

    let token_hash = sha256::digest(
        format!(
            "{}{}",
            token,
            secrets
                .get("ACCESS_TOKEN_HASH_SALT")
                .ok_or("No secret ACCESS_TOKEN_HASH_SALT provided")?
        )
        .as_bytes(),
    );

    let expires_at = now
        .checked_add_signed(chrono::Duration::hours(6))
        .unwrap()
        .to_rfc3339();

    client
        .put_item()
        .table_name(&table)
        .set_item(Some(HashMap::from([
            ("id".to_string(), S(format!("accesstoken#{}", token_hash))),
            ("email".to_string(), S(email.clone())),
            ("created_at".to_string(), S(now.to_rfc3339())),
            ("expires_at".to_string(), S(expires_at.clone())),
        ])))
        .send()
        .await?;

    user::UserBuilder::new(&client, &table)
        .create(email.clone())
        .await
        .wrap_err("Faile to create user")?;

    crate::json::response(
        json!({"email": email, "token": token_hash, "expiresAt": expires_at}),
        None,
    )
}
