use aws_config::{Region, SdkConfig};
use aws_sdk_dsql::auth_token::{AuthToken, AuthTokenGenerator, Config as DsqlConfig};
use eyre::Context;
use lambda_runtime::Error;
use log::{error, info};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug)]
pub struct SqlDb {
    /// Public endpoint for the DSQL cluster
    endpoint: String,

    /// Username used to access the database
    username: String,

    /// Password for DSQL cluster access
    password: Arc<RwLock<String>>,

    /// AWS SDK configuration
    config: SdkConfig,

    /// SSL mode for the connection
    ssl_mode: String,
}

/// SQL DB configuration details
impl SqlDb {
    pub async fn new(cluster_id: &str, username: &str, config: &SdkConfig) -> Result<Self, Error> {
        let region = config.region().unwrap_or(&Region::new("us-east-1")).clone();
        let endpoint = format!("{}.dsql.{}.on.aws", cluster_id, region.as_ref());
        info!("Initializing SQL DB connection: {}", endpoint);

        let password = fetch_dsql_password(&endpoint, config)
            .await
            .map_err(|err| {
                eyre::eyre!(
                    "Failed to fetch auth token for cluster {cluster_id}: {username}: {:?}",
                    err
                )
            })?;

        let database = Self {
            endpoint,
            password: Arc::new(RwLock::new(password.to_string())),
            username: username.to_string(),
            config: config.clone(),
            ssl_mode: "verify-full".to_string(), // Strictly require TLS for postgres connections
        };

        Ok(database)
    }

    // Creates a local SQL DB instance using provided connection string
    pub async fn new_local(connection_string: &str, config: &SdkConfig) -> Result<Self, Error> {
        // Parse as regular URL postgres://username:password@localhost:5432/dbname
        let url = Url::parse(connection_string).wrap_err("Failed to parse connection string")?;
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

        let database = Self {
            username: url.username().to_string(),
            password: Arc::new(RwLock::new(url.password().unwrap_or("").to_string())),
            endpoint: url.host_str().unwrap_or("localhost").to_string(),
            config: config.clone(),

            ssl_mode: params
                .get("ssl_mode")
                .unwrap_or(&String::from("disable"))
                .to_owned(),
        };

        Ok(database)
    }

    pub fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    pub fn username(&self) -> String {
        self.username.clone()
    }

    pub fn password(&self) -> String {
        self.password.read().unwrap().to_string()
    }

    pub fn port(&self) -> u16 {
        5432
    }

    pub fn database(&self) -> String {
        "postgres".to_string()
    }

    pub fn connection_string(&self) -> String {
        let password = utf8_percent_encode(self.password().as_str(), NON_ALPHANUMERIC).to_string();

        format!(
            "postgresql://{username}:{password}@{endpoint}:{port}/{database}?sslmode={ssl_mode}",
            username = self.username(),
            endpoint = self.endpoint(),
            port = self.port(),
            database = self.database(),
            ssl_mode = self.ssl_mode,
        )
    }

    /// Spawns a task that periodically refreshes the authentication token.
    ///
    /// The token is fetched every 10 minutes, but the function implements an exponential
    /// backoff mechanism to retry the fetch operation in case of failure.
    /// Note: The first token refresh happens after a 10-minute delay.
    pub fn spawn_password_refresh(self) -> Self {
        let config = self.config.clone();
        let cluster_endpoint = self.endpoint.clone();
        let password = self.password.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60 * 10));
            interval.tick().await; // The first tick happens immediately, skip it

            // Exponential backoff state (retry-until-success)
            let mut backoff = Duration::from_secs(1);
            let max_backoff = Duration::from_secs(60);

            loop {
                interval.tick().await;

                match fetch_dsql_password(&cluster_endpoint, &config).await {
                    Ok(token) => {
                        *password.write().unwrap() = token.to_string();
                    }

                    Err(err) => {
                        error!("Failed to refresh auth token: {err}");

                        // Retry-until-success with bounded exponential backoff
                        tokio::time::sleep(backoff).await;
                        backoff = std::cmp::min(backoff.saturating_mul(2), max_backoff);

                        // Schedule the next attempt immediately
                        interval.reset_immediately()
                    }
                }
            }
        });

        self
    }
}

async fn fetch_dsql_password(endpoint: &str, config: &SdkConfig) -> Result<AuthToken, Error> {
    let dsql_config = DsqlConfig::builder()
        .hostname(endpoint)
        .build()
        .map_err(|err| eyre::eyre!("Failed to build DSQL config: {:?}", err))?;

    let signer = AuthTokenGenerator::new(dsql_config);
    signer.db_connect_auth_token(config).await
}
