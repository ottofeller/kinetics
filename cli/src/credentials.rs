use crate::error::Error;
use chrono::{DateTime, Utc};
use eyre::Context;
use serde_json::json;
use std::path::{Path, PathBuf};

/// Credentials to be used with API
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Credentials {
    #[serde(skip)]
    path: PathBuf,

    pub(crate) email: String,
    pub(crate) token: String,
    pub(crate) expires_at: DateTime<Utc>,
}

/// Managing credentials file
impl Credentials {
    /// Initialize from the path to credentials file
    pub(crate) fn new(path: &Path) -> eyre::Result<Self> {
        let mut credentials = serde_json::from_str::<crate::credentials::Credentials>(
            &std::fs::read_to_string(path)
                .or_else(|_| {
                    let default =
                        json!({ "email": "", "token": "", "expiresAt": "2000-01-01T00:00:00Z" })
                            .to_string();

                    std::fs::write(path, default.clone())?;
                    eyre::Ok(default)
                })
                .unwrap_or_default(),
        )
        .wrap_err(Error::new(
            "Could not parse credentials file",
            Some(&format!("Delete {} and try again", path.display())),
        ))?;

        credentials.path = path.to_path_buf();
        Ok(credentials)
    }

    /// Credentials are presented for the email and are not expired
    pub fn is_valid(&self, email: &str) -> bool {
        !self.token.is_empty()
            && self.expires_at.timestamp() > Utc::now().timestamp()
            && self.email == email
    }

    /// Update credentials file with new email, token, and expiration time
    pub fn write(&mut self, credentials: Credentials) -> eyre::Result<()> {
        self.email = credentials.email;
        self.token = credentials.token;
        self.expires_at = credentials.expires_at;

        std::fs::write(self.path.clone(), json!(self).to_string()).wrap_err(Error::new(
            "Failed to store credentials",
            Some("File system issue, check the file permissions in ~/.kinetics/.credentials"),
        ))?;

        Ok(())
    }
}
