use crate::tools::sqldb::SqlDb;
use aws_config::SdkConfig;
use lambda_runtime::Error;
use std::collections::HashMap;

/// Runtime lambda configuration
///
/// Config is passed into the Lambda handler
#[derive(Clone, Debug)]
pub struct Config {
    pub db: HashMap<String, SqlDb>,
}

impl Config {
    pub async fn new(config: &SdkConfig) -> Result<Self, Error> {
        let mut all_db = HashMap::new();
        let cluster_id = std::env::var("KINETICS_SQLDB_CLUSTER_ID")?;

        for (key, value) in std::env::vars() {
            if !key.starts_with("KINETICS_SQLDB_USER_") {
                continue;
            }

            let db = SqlDb::new(&cluster_id, &value, config).await?;
            let key = key.replace("KINETICS_SQLDB_USER_", "");
            all_db.insert(key, db);
        }

        Ok(Self { db: all_db })
    }
}
