use crate::sqldb::SqlDb;
use aws_config::SdkConfig;
use lambda_runtime::Error;

/// Runtime lambda configuration
///
/// Config is passed into the Lambda handler
#[derive(Clone, Debug)]
pub struct Config {
    pub db: SqlDb,
}

impl Config {
    pub async fn new(config: &SdkConfig) -> Result<Self, Error> {
        let cluster_id = std::env::var("KINETICS_SQLDB_CLUSTER_ID");
        let user = std::env::var("KINETICS_SQLDB_USER");

        // If both cluster_id and user are set, use them to connect to sqldb
        if let (Ok(cluster_id), Ok(user)) = (cluster_id, user) {
            return Ok(Self {
                db: SqlDb::new(&cluster_id, &user, config).await?,
            });
        }

        let connection_string = std::env::var("KINETICS_SQLDB_LOCAL_CONNECTION_STRING")
            // Add any valid connection string as default if sqldb is not enabled
            .unwrap_or("postgres://user:password@localhost:5432/postgres".to_string());

        Ok(Self {
            db: SqlDb::new_local(&connection_string, config).await?,
        })
    }
}
