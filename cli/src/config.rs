use crate::error::Error;
use chrono::{DateTime, Utc};
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug)]
pub(crate) struct Config<'a> {
    pub(crate) api_base: &'a str,
    pub(crate) build_path: &'a str,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub(crate) fn build_config() -> Result<&'static Config<'static>, Error> {
    CONFIG.get_or_try_init(|| {
        let api_base =
            option_env!("KINETICS_API_BASE").unwrap_or("https://backend.usekinetics.com/");

        let home_dir = match std::env::var("HOME") {
            Ok(home_dir) => home_dir,
            Err(_) => {
                log::error!("Failed to get $HOME");

                return Err(Error::new(
                    "$HOME is missing",
                    Some("Your shell might not be supported."),
                ));
            }
        };

        let build_path = Box::leak(
            Path::new(&home_dir)
                .join(".kinetics")
                .display()
                .to_string()
                .into_boxed_str(),
        );

        {
            Ok(Config {
                api_base,
                build_path,
            })
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
    format!("{}{}", build_config().unwrap().api_base, path)
}
