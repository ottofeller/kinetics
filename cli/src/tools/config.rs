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
        let cluster_id = std::env::var("KINETICS_SQLDB_CLUSTER_ID")?;
        let user = std::env::var("KINETICS_SQLDB_USER")?;

        Ok(Self {
            db: SqlDb::new(&cluster_id, &user, config).await?,
        })
    }
}
