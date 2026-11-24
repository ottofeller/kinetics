mod config;
mod cron;
mod endpoint;
mod worker;

#[cfg(test)]
#[path = "tests/runtime.rs"]
mod tests;

pub use config::{
    RUNTIME_CONFIG_KEY_ENV, RUNTIME_CONFIG_TABLE_ENV, RUNTIME_CONFIG_VERSION, RuntimeConfig,
};

/// Dependencies used by the exported entry point macros via `$crate`.
#[doc(hidden)]
pub mod dependencies {
    pub use {aws_lambda_events, http, lambda_http, lambda_runtime, serde_json, tokio, tower};
}

use super::{
    config::{Config, EndpointConfig},
    secrets,
};
use aws_config::SdkConfig;
use aws_sdk_dynamodb::Client;
use lambda_runtime::Error;
use std::{collections::HashMap, sync::OnceLock};
use tokio::sync::OnceCell;

#[derive(Clone, Copy, Debug)]
pub enum Environment {
    Local,
    Lambda,
}

pub struct Runtime {
    pub secrets: HashMap<String, String>,
    pub config: Config,
}

impl Runtime {
    pub async fn load(
        environment: Environment,
        endpoint: Option<EndpointConfig>,
    ) -> Result<Self, Error> {
        match environment {
            Environment::Local => Ok(Self {
                secrets: secrets::from_env(std::env::vars()),
                config: Config::from_local(endpoint).await?,
            }),
            Environment::Lambda => {
                let sdk = sdk_config().await;
                let runtime = RuntimeConfig::load(sdk).await?;
                let (secrets, mut config) = tokio::try_join!(
                    async {
                        secrets::load(dynamodb(sdk), runtime)
                            .await
                            .map_err(Error::from)
                    },
                    Config::from_runtime(sdk, runtime, endpoint),
                )?;
                config.db = config.db.spawn_password_refresh();
                Ok(Self { secrets, config })
            }
        }
    }
}

static SDK_CONFIG: OnceCell<SdkConfig> = OnceCell::const_new();
static DYNAMODB: OnceLock<Client> = OnceLock::new();

pub(super) async fn sdk_config() -> &'static SdkConfig {
    SDK_CONFIG
        .get_or_init(|| aws_config::load_defaults(aws_config::BehaviorVersion::latest()))
        .await
}

fn dynamodb(sdk: &SdkConfig) -> &'static Client {
    DYNAMODB.get_or_init(|| Client::new(sdk))
}
