use crate::sqldb::SqlDb;
use crate::tools::runtime::RuntimeConfig;
use aws_config::SdkConfig;
use lambda_runtime::Error;

/// Configuration of an endpoint lambda
#[derive(Clone, Debug)]
pub struct EndpointConfig {
    pub url_pattern: String,
}

impl EndpointConfig {
    pub fn new(url_pattern: &str) -> Self {
        Self {
            url_pattern: url_pattern.to_owned(),
        }
    }
}

impl std::fmt::Display for EndpointConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "EndpointConfig {{ url_pattern: \"{}\".to_string() }}",
            self.url_pattern
        )
    }
}

/// Runtime lambda configuration
///
/// Config is passed into the Lambda handler
#[derive(Clone, Debug)]
pub struct Config {
    pub db: SqlDb,
    endpoint: Option<EndpointConfig>,
}

impl Config {
    pub(super) async fn from_runtime(
        config: &SdkConfig,
        runtime: &RuntimeConfig,
        endpoint: Option<EndpointConfig>,
    ) -> Result<Self, Error> {
        let db = SqlDb::new(&runtime.sqldb_cluster_id, &runtime.sqldb_user, config).await?;
        Ok(Self { db, endpoint })
    }

    pub(super) async fn from_local(endpoint: Option<EndpointConfig>) -> Result<Self, Error> {
        // Config.db is required even when local invocation does not provision a database.
        let connection_string = std::env::var("KINETICS_SQLDB_LOCAL_CONNECTION_STRING")
            .unwrap_or_else(|_| "postgres://user:password@localhost:5432/postgres".into());

        Ok(Self {
            db: SqlDb::new_local(&connection_string).await?,
            endpoint,
        })
    }

    pub fn url_pattern(&self) -> Option<&String> {
        self.endpoint.as_ref().map(|e| &e.url_pattern)
    }
}
