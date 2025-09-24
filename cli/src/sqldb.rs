use aws_config::{Region, SdkConfig};
use aws_sdk_dsql::auth_token::{AuthToken, AuthTokenGenerator, Config as DsqlConfig};
use lambda_runtime::Error;
use log::error;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct SqlDb {
    /// Public endpoint for the DSQL cluster
    endpoint: String,

    /// Username used to access the database
    username: String,

    /// Password for DSQL cluster access
    password: Arc<RwLock<AuthToken>>,

    /// AWS SDK configuration
    config: SdkConfig,
}

/// SQL DB configuration details
impl SqlDb {
    pub async fn new(cluster_id: &str, username: &str, config: &SdkConfig) -> Result<Self, Error> {
        let region = config.region().unwrap_or(&Region::new("us-east-1")).clone();
        let endpoint = format!("{}.dsql.{}.on.aws", cluster_id, region.as_ref());
        let password = fetch_dsql_password(&endpoint, config).await?;

        let database = Self {
            endpoint,
            password: Arc::new(RwLock::new(password)),
            username: username.to_string(),
            config: config.clone(),
        };

        // Refresh the auth password in the background
        database.spawn_password_refresh();

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
            "postgresql://{username}:{password}@{endpoint}:{port}/{database}?sslmode=verify-full",
            username = self.username(),
            endpoint = self.endpoint(),
            port = self.port(),
            database = self.database(),
        )
    }

    /// Spawns a task that periodically refreshes the authentication token.
    ///
    /// The token is fetched every 10 minutes, but the function implements an exponential
    /// backoff mechanism to retry the fetch operation in case of failure.
    /// Note: The first token refresh happens after a 10-minute delay.
    fn spawn_password_refresh(&self) {
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
                        *password.write().unwrap() = token;
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
    }
}

async fn fetch_dsql_password(endpoint: &str, config: &SdkConfig) -> Result<AuthToken, Error> {
    let signer = AuthTokenGenerator::new(DsqlConfig::builder().hostname(endpoint).build()?);
    signer.db_connect_auth_token(config).await
}
