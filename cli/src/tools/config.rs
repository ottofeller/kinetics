use crate::sqldb::SqlDb;
use aws_config::SdkConfig;
use kinetics_parser::Endpoint;
use lambda_runtime::Error;

/// Configuration of an endpoint lambda
#[derive(Clone, Debug)]
pub struct EndpointConfig {
    pub url_pattern: Option<String>,
}

impl EndpointConfig {
    pub fn new(url_pattern: &Option<String>) -> Self {
        Self {
            url_pattern: url_pattern.to_owned(),
        }
    }
}

impl std::fmt::Display for EndpointConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "EndpointConfig {{ url_pattern: {}}}",
            match self.url_pattern.clone() {
                Some(url_pattern) => format!("Some(\"{url_pattern}\".to_string())"),
                None => "None".into(),
            }
        )
    }
}

/// Runtime lambda configuration
///
/// Config is passed into the Lambda handler
#[derive(Clone, Debug)]
pub struct Config {
    pub db: SqlDb,
    pub endpoint: Option<EndpointConfig>,
}

impl Config {
    pub async fn new(config: &SdkConfig, endpoint: Option<EndpointConfig>) -> Result<Self, Error> {
        let cluster_id = std::env::var("KINETICS_SQLDB_CLUSTER_ID");
        let user = std::env::var("KINETICS_SQLDB_USER");

        // If both cluster_id and user are set, use them to connect to sqldb
        if let (Ok(cluster_id), Ok(user)) = (cluster_id, user) {
            return Ok(Self {
                db: SqlDb::new(&cluster_id, &user, config).await?,
                endpoint,
            });
        }

        let connection_string = std::env::var("KINETICS_SQLDB_LOCAL_CONNECTION_STRING")
            // Add any valid connection string as default if sqldb is not enabled
            .unwrap_or("postgres://user:password@localhost:5432/postgres".to_string());

        Ok(Self {
            db: SqlDb::new_local(&connection_string, config).await?,
            endpoint,
        })
    }
}
