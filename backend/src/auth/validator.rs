use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use eyre::{Context, ContextCompat};
use lambda_http::Request;
pub struct Validator;

/// Check that the token in the Authorization header is valid and not expired
pub async fn is_authorized(request: &Request, table_name: &str) -> eyre::Result<bool> {
    let header = request.headers().get("Authorization");

    if header.is_none() {
        eprint!("The auth header is missing");
        return Ok(false);
    }

    let token = header.unwrap().to_str()?.to_string();

    if token.is_empty() {
        eprint!("The access token is empty");
        return Ok(false);
    }

    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Client::new(&config);

    let result = client
        .get_item()
        .table_name(table_name)
        .key("id", AttributeValue::S(format!("accesstoken#{}", token)))
        .send()
        .await?;

    if result.item().is_none() {
        eprint!("The access token was not found in DB");
        return Ok(false);
    }

    let expires_at = match result
        .item()
        .unwrap()
        .get("expires_at")
        .wrap_err("expires_at is missing")?
        .as_s()
    {
        Ok(expires_at) => chrono::DateTime::parse_from_rfc3339(expires_at)
            .wrap_err("Failed to parse date string in expires_at")?,

        Err(_) => return Err(eyre::eyre!("Wrong string format in expires_at attr")),
    };

    let now = chrono::Utc::now();

    if expires_at.timestamp() < now.timestamp() {
        eprint!("The access token expired");
        return Ok(false);
    }

    Ok(result.item().is_some())
}
