use crate::tools::database::SqlDb;
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

        for (key, cluster_id) in std::env::vars() {
            if !key.starts_with("KINETICS_SQLDB_") {
                continue;
            }

            let db_name = key.replace("KINETICS_SQLDB_", "");
            let db = SqlDb::new(&cluster_id, config).await?;
            all_db.insert(db_name, db);
        }

        Ok(Self { db: all_db })
    }
}
