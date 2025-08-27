use crate::tools::database::Database;
use aws_config::SdkConfig;
use lambda_runtime::Error;
use std::collections::HashMap;

/// Runtime lambda configuration
///
/// Config is passed into the Lambda handler
#[derive(Clone, Debug)]
pub struct KineticsConfig {
    pub db: HashMap<String, Database>,
}

impl KineticsConfig {
    pub async fn new(config: &SdkConfig) -> Result<Self, Error> {
        let mut all_db = HashMap::new();

        for (key, cluster_id) in std::env::vars() {
            if !key.starts_with("KINETICS_DATABASE_") {
                continue;
            }

            let db_name = key.replace("KINETICS_DATABASE_", "");
            let db = Database::new(&cluster_id, config).await?;
            all_db.insert(db_name, db);
        }

        Ok(Self { db: all_db })
    }
}
