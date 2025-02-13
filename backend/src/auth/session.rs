use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use chrono::{DateTime, Utc};
use eyre::{Context, ContextCompat};
use lambda_http::Request;

/// User's session
#[derive(Clone, Default)]
pub struct Session {
    expires_at: Option<chrono::DateTime<Utc>>,
    email: Option<String>,
    token: Option<String>,
}

impl Session {
    /// Try to find a session record in DB, instantiate the struct if found
    pub async fn new(request: &Request, table_name: &str) -> eyre::Result<Self> {
        let header = request.headers().get("Authorization");

        if header.is_none() {
            eprint!("The auth header is missing");
            return Ok(Session::default());
        }

        let token = header.unwrap().to_str()?.to_string();

        if token.is_empty() {
            eprint!("The access token is empty");
            return Ok(Session::default());
        }

        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let client = Client::new(&config);

        let result = client
            .get_item()
            .table_name(table_name)
            .key("id", AttributeValue::S(format!("accesstoken#{}", token)))
            .send()
            .await
            .wrap_err("DB request failed")?;

        if result.item().is_none() {
            eprint!("The access token was not found in DB");
            return Ok(Session::default());
        }

        let item = result.item().unwrap();

        let expires_at: DateTime<Utc> = match item
            .get("expires_at")
            .wrap_err("expires_at is missing")?
            .as_s()
        {
            Ok(expires_at) => DateTime::parse_from_rfc3339(expires_at)
                .wrap_err("Failed to parse date string in expires_at")?
                .to_utc(),

            Err(_) => return Err(eyre::eyre!("Wrong string format in expires_at attr")),
        };

        let now = chrono::Utc::now();

        if expires_at.timestamp() < now.timestamp() {
            eprint!("The access token expired");
            return Ok(Session::default());
        }

        Ok(Session {
            expires_at: Some(expires_at),

            email: Some(
                item.get("email")
                    .wrap_err("email is missing")?
                    .as_s()
                    .unwrap()
                    .to_owned(),
            ),

            token: Some(token),
        })
    }

    /// Simply check of token is Some()
    ///
    /// The only reason why it can be None is that it is invalid or expired.
    pub fn is_valid(&self) -> bool {
        let now = chrono::Utc::now();

        self.token.is_some()
            && self.expires_at.is_some()
            && self.expires_at.unwrap().timestamp() > now.timestamp()
    }

    /// Generate unique username out of email, suitable to be used in CloudFormation
    pub fn username(&self) -> String {
        self.email
            .clone()
            .unwrap()
            .replace("@", "AT")
            .replace(".", "DOT")
    }
}
