use chrono::{DateTime, Utc};
use eyre::Context;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const DIRECT_DEPLOY_ENABLED: bool = cfg!(feature = "enable-direct-deploy");

#[derive(Debug)]
pub(crate) struct Config<'a> {
    pub(crate) api_base: &'a str,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub(crate) fn build_config() -> &'static Config<'static> {
    CONFIG.get_or_init(|| {
        let api_base =
            option_env!("KINETICS_API_BASE").unwrap_or("https://backend.usekinetics.com/");

        {
            Config { api_base }
        }
    })
}

/// Credentials to be used with API
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    pub email: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

pub fn api_url(path: &str) -> String {
    format!("{}{}", build_config().api_base, path)
}

pub fn build_path() -> eyre::Result<PathBuf> {
    Ok(Path::new(&std::env::var("HOME").wrap_err("Can not read HOME env var")?).join(".kinetics"))
}
