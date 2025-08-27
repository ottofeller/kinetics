use aws_config::{Region, SdkConfig};
use aws_sdk_dsql::auth_token::{AuthToken, AuthTokenGenerator, Config as DsqlConfig};
use lambda_runtime::Error;
use log::error;
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Database {
    // Unique identifier for the DSQL cluster
    cluster_id: String,

    /// Password for DSQL cluster access
    password: Arc<RwLock<AuthToken>>,

    /// AWS SDK configuration
    config: SdkConfig,
}

impl Database {
    pub async fn new(cluster_id: &str, config: &SdkConfig) -> Result<Self, Error> {
        let password = fetch_dsql_password(cluster_id, config).await?;

        let database = Self {
            cluster_id: cluster_id.to_string(),
            password: Arc::new(RwLock::new(password)),
            config: config.clone(),
        };

        // Refresh the auth token in the background
        database.start_token_refresh();

        Ok(database)
    }

    pub fn endpoint(&self) -> String {
        generate_cluster_endpoint(&self.cluster_id, &self.config)
    }

    pub fn username(&self) -> String {
        "admin".to_string()
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
        format!(
            "postgresql://{username}:{password}@{endpoint}:{port}/{database}",
            username = self.username(),
            password = self.password(),
            endpoint = self.endpoint(),
            port = self.port(),
            database = self.database(),
        )
    }

    fn start_token_refresh(&self) {
        let config = self.config.clone();
        let cluster_id = self.cluster_id.clone();
        let password = self.password.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60 * 10));
            interval.tick().await; // The first tick happens immediately, skip it

            // Exponential backoff state (retry-until-success)
            let mut backoff = Duration::from_secs(1);
            let max_backoff = Duration::from_secs(60);

            loop {
                interval.tick().await;

                match fetch_dsql_password(&cluster_id, &config).await {
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

fn generate_cluster_endpoint(cluster_id: &str, config: &SdkConfig) -> String {
    let region = config.region().unwrap_or(&Region::new("us-east-1")).clone();
    // See https://docs.aws.amazon.com/general/latest/gr/dsql.html
    format!("{}.dsql.{}.on.aws", cluster_id, region.as_ref())
}

async fn fetch_dsql_password(cluster_id: &str, config: &SdkConfig) -> Result<AuthToken, Error> {
    let endpoint = generate_cluster_endpoint(cluster_id, config);
    let signer = AuthTokenGenerator::new(DsqlConfig::builder().hostname(endpoint).build()?);
    signer.db_connect_admin_auth_token(config).await
}
