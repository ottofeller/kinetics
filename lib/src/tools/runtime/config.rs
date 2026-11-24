use super::dynamodb;
use crate::tools::features::Features;
use aws_config::SdkConfig;
use aws_sdk_dynamodb::{Client, types::AttributeValue};
use eyre::{OptionExt, WrapErr};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tokio::sync::OnceCell;

pub const RUNTIME_CONFIG_VERSION: u32 = 2;
pub const RUNTIME_CONFIG_TABLE_ENV: &str = "KINETICS_RUNTIME_CONFIG_TABLE_NAME";
pub const RUNTIME_CONFIG_KEY_ENV: &str = "KINETICS_RUNTIME_CONFIG_KEY";

static RUNTIME_CONFIG: OnceCell<RuntimeConfig> = OnceCell::const_new();

/// Shared configuration written by the backend and read by cloud Lambda runtimes.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub features: Features,
    pub owner_id: String,
    pub cloud_account_id: String,
    pub secrets_table_name: String,
    pub project_scope: String,
    pub package_scope: String,
    pub sqldb_cluster_id: String,
    pub sqldb_user: String,
    /// Queue logical names to physical names mapping.
    pub queues: BTreeMap<String, String>,
    /// Project storage bucket.
    pub storage_bucket: String,
    /// Project storage key prefix.
    pub storage_prefix: String,
}

impl RuntimeConfig {
    pub async fn load(sdk: &SdkConfig) -> eyre::Result<&'static Self> {
        RUNTIME_CONFIG
            .get_or_try_init(|| async {
                let table = std::env::var(RUNTIME_CONFIG_TABLE_ENV)
                    .wrap_err("Runtime configuration table is not set")?;
                let key = std::env::var(RUNTIME_CONFIG_KEY_ENV)
                    .wrap_err("Runtime configuration key is not set")?;
                Self::fetch(dynamodb(sdk), &table, &key).await
            })
            .await
    }

    async fn fetch(client: &Client, table_name: &str, key: &str) -> eyre::Result<Self> {
        let response = client
            .get_item()
            .table_name(table_name)
            .key("id", AttributeValue::S(key.to_owned()))
            .send()
            .await
            .wrap_err("Failed to load runtime configuration")?;
        let item = response
            .item
            .ok_or_eyre("Runtime configuration not found")?;
        let config: Self =
            serde_dynamo::from_item(item).wrap_err("Invalid runtime configuration")?;

        eyre::ensure!(
            config.schema_version == RUNTIME_CONFIG_VERSION,
            "Unsupported runtime configuration schema version: {}",
            config.schema_version
        );

        Ok(config)
    }
}

#[cfg(test)]
#[path = "../tests/runtime_config.rs"]
mod tests;
