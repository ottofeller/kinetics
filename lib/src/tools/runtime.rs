use aws_config::SdkConfig;
use eyre::{Context, OptionExt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;
use tokio::sync::OnceCell;

/// The environment variable that contains the URI of the runtime configuration.
pub const RUNTIME_CONFIG_ENV_URI: &str = "KINETICS_RUNTIME_CONFIG_URI";

pub const RUNTIME_CONFIG_VERSION: u32 = 1;

/// Shared JSON configuration written by the backend and read by cloud Lambda runtimes.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeConfig {
    pub schema_version: u32,
    pub owner_id: String,
    pub project_name: String,
    pub cloud_account_id: String,
    pub sqldb_cluster_id: String,
    pub sqldb_user: String,
    /// SSM parameter names
    pub secrets_names: Vec<String>,

    /// Queue logical names to physical names mapping.
    pub queues: BTreeMap<String, String>,
}

static RUNTIME_CONFIG: OnceCell<RuntimeConfig> = OnceCell::const_new();

/// Parsed S3 location of the runtime configuration.
struct RuntimeConfigUri {
    bucket: String,
    key: String,
}

impl FromStr for RuntimeConfigUri {
    type Err = eyre::Report;

    fn from_str(uri: &str) -> Result<Self, Self::Err> {
        let (bucket, key) = uri
            .strip_prefix("s3://")
            .and_then(|location| location.split_once('/'))
            .filter(|(bucket, key)| !bucket.is_empty() && !key.is_empty())
            .ok_or_eyre("Runtime configuration URI must have the form s3://bucket/key")?;

        Ok(Self {
            bucket: bucket.to_string(),
            key: key.to_string(),
        })
    }
}

/// Load the required cloud configuration once per Lambda execution environment.
pub async fn load(sdk_config: &SdkConfig) -> eyre::Result<&'static RuntimeConfig> {
    RUNTIME_CONFIG
        .get_or_try_init(|| async {
            let uri = std::env::var(RUNTIME_CONFIG_ENV_URI).wrap_err_with(|| {
                format!("{RUNTIME_CONFIG_ENV_URI} must be set for cloud invocation")
            })?;

            let uri = uri.parse::<RuntimeConfigUri>()?;

            let response = aws_sdk_s3::Client::new(sdk_config)
                .get_object()
                .bucket(uri.bucket)
                .key(uri.key)
                .send()
                .await
                .wrap_err("Failed to fetch runtime configuration from S3")?;

            let body = response
                .body
                .collect()
                .await
                .wrap_err("Failed to read runtime configuration from S3")?;

            let runtime_config: RuntimeConfig = serde_json::from_slice(&body.into_bytes())
                .map_err(|_| eyre::eyre!("Invalid runtime configuration JSON"))?;

            if runtime_config.schema_version != RUNTIME_CONFIG_VERSION {
                eyre::bail!(
                    "Unsupported runtime configuration schema version: {}",
                    runtime_config.schema_version
                );
            }

            Ok(runtime_config)
        })
        .await
}
