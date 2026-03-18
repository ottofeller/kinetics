use crate::api::auth;
use crate::config::{api_url, build_config};
use crate::error::Error;
use chrono::{DateTime, Utc};
use eyre::{Context, OptionExt};
use keyring::Entry;
use reqwest::StatusCode;
use serde_json::json;
use std::path::{Path, PathBuf};
use users::{get_current_uid, get_user_by_uid};

/// Credentials to be used with API
#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct Credentials {
    #[serde(skip)]
    pub(crate) path: PathBuf,

    pub(crate) email: String,
    pub(crate) token: String,
    pub(crate) expires_at: DateTime<Utc>,
}

/// Managing credentials file
impl Credentials {
    /// Fetch email and expires_at associated with the token
    async fn fetch_info(token: &str) -> eyre::Result<auth::info::Response> {
        let url = "/auth/info";

        // Can't use internal client here, as it will create recursion
        let result = reqwest::Client::new()
            .get(api_url(url))
            .header("Authorization", token)
            .send()
            .await
            .inspect_err(|e| log::error!("Request to /auth/info failed: {e:?}"))?;

        let status = result.status();
        let text = result.text().await?;
        log::debug!("Got status from {url}: {status}");
        log::debug!("Got response from {url}: {text}");

        if status != StatusCode::OK {
            log::error!("Auth info request status is not OK");
            return Err(eyre::eyre!("Status is not 200"));
        }

        Ok(serde_json::from_str(&text)
            .inspect_err(|e| log::error!("Could not parse auth info response: {e:?}"))?)
    }

    /// Initialize from the path to credentials file
    ///
    /// First checks environment variable, if not found, reads from file
    pub async fn new() -> eyre::Result<Self> {
        let config = build_config()?;
        let path = Path::new(config.credentials_path);

        if let Ok(credentials) = Self::from_env().await.inspect_err(|error| {
            log::info!("Failed to read credentials from env, skipping: {error}")
        }) {
            log::info!("Using credentials from env var");
            return Ok(credentials);
        }

        // Use keyring second (high priority)
        if let Ok(credentials) = Self::from_keyring().inspect_err(|error| {
            log::info!("Failed to get credentials from keyring, , skipping: {error}")
        }) {
            log::info!("Using credentials from keyring");
            return Ok(credentials);
        };

        // Fall back to reading from file
        log::info!("Using credentials from {}", path.to_string_lossy());
        let mut credentials = Self::from_file()?;
        credentials.path = path.to_path_buf();
        Ok(credentials)
    }

    /// Credentials are presented for the email and are not expired
    pub fn is_valid(&self) -> bool {
        !self.token.is_empty() && self.expires_at.timestamp() > Utc::now().timestamp()
    }

    /// Update credentials file with new email, token, and expiration time
    pub fn write(&mut self, credentials: Credentials) -> eyre::Result<()> {
        self.email = credentials.email;
        self.token = credentials.token;
        self.expires_at = credentials.expires_at;

        // Try writing to keyring first
        if {
            log::info!("Write token to secure store");
            Self::keyring_entry()?.set_password(&json!(self).to_string())?;
            Ok::<(), ()>(())
        }
        .is_ok()
        {
            return Ok(());
        }

        // Fallback to a file
        log::info!("Write token to file");

        std::fs::write(self.path.clone(), json!(self).to_string()).wrap_err(Error::new(
            "Failed to store credentials",
            Some("File system issue, check the file permissions in ~/.kinetics/.credentials"),
        ))?;

        Ok(())
    }

    /// Pull an entry for kinetics token
    /// from platform specific secure store
    fn keyring_entry() -> eyre::Result<Entry> {
        let user = get_user_by_uid(get_current_uid()).ok_or_eyre("Failed getting current user")?;
        let username = user.name().to_str().ok_or_eyre("Invalid username")?;
        log::info!("Get token entry from secure store for user {username}");
        let entry = Entry::new("kinetics:api-token", username)?;
        Ok(entry)
    }

    /// Init credentials from platform specific secure store
    fn from_keyring() -> eyre::Result<Credentials> {
        let entry = &Self::keyring_entry()?;
        log::info!("Get token from secure store");
        let credentials: Credentials = serde_json::from_str(&entry.get_password()?)?;
        Ok(credentials)
    }

    /// Init from API token set in the the env
    async fn from_env() -> eyre::Result<Credentials> {
        let config = build_config()?;
        log::info!("Using credentials from env {}", config.credentials_env);
        let path = Path::new(config.credentials_path);

        let token = std::env::var(config.credentials_env).wrap_err(Error::new(
            "Could not parse credentials file",
            Some(&format!("Delete {} and try again", path.display())),
        ))?;

        // Fetch token info from backend
        let info = Self::fetch_info(&token).await.wrap_err(Error::new(
            "Failed to fetch auth info",
            Some(&format!(
                "Check if your {} is valid.",
                config.credentials_env
            )),
        ))?;

        return Ok(Credentials {
            path: path.to_path_buf(),
            email: info.email,
            token,
            expires_at: info.expires_at,
        });
    }

    /// Init from json file
    fn from_file() -> eyre::Result<Credentials> {
        let config = build_config()?;
        let path = Path::new(config.credentials_path);

        serde_json::from_str::<crate::credentials::Credentials>(
            &std::fs::read_to_string(path)
                // Create credentials file with empty defaults if it's missing
                .or_else(|_| {
                    let default =
                        json!({ "email": "", "token": "", "expires_at": "2000-01-01T00:00:00Z" })
                            .to_string();
                    if let Some(dir) = path.parent() {
                        if !dir.exists() {
                            std::fs::create_dir_all(dir).wrap_err(format!(
                                "Failed to create dir \"{:?}\" to store credential file",
                                dir
                            ))?;
                        }
                    };

                    eyre::Ok(default)
                })
                .unwrap_or_default(),
        )
        .wrap_err(Error::new(
            "Could not parse credentials file",
            Some(&format!("Delete {} and try again", path.display())),
        ))
    }
}
