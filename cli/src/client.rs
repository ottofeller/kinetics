use chrono::Utc;
use eyre::Context;
use serde_json::json;
use std::path::Path;
static API_BASE: &str = "https://backend.usekinetics.com";

pub struct Client {
    access_token: String,
    client: reqwest::Client,
}

impl Client {
    pub fn new(is_directly: &bool) -> eyre::Result<Self> {
        if *is_directly {
            return Ok(Client {
                access_token: "".into(),
                client: reqwest::Client::new(),
            });
        }

        let path = Path::new(&crate::skypath()?).join(".credentials");

        let credentials = serde_json::from_str::<crate::Credentials>(
            &std::fs::read_to_string(path.clone())
                .or_else(|_| {
                    let default =
                        json!({ "email": "", "token": "", "expiresAt": "2000-01-01T00:00:00Z" })
                            .to_string();

                    std::fs::write(path.clone(), default.clone())?;
                    eyre::Ok(default.into())
                })
                .unwrap_or_default(),
        )
        .wrap_err("Credentials stored in a wrong format")?;

        // If credentials expired — request to re-login
        if credentials.expires_at.timestamp() <= Utc::now().timestamp() {
            return Err(eyre::eyre!("Credentials expired, please re-login."));
        }

        Ok(Client {
            access_token: credentials.token,
            client: reqwest::Client::new(),
        })
    }

    fn url(path: &str) -> String {
        format!("{}{}", API_BASE, path)
    }

    /// A POST request with the Authorization header
    pub fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .post(Self::url(path))
            .header("Authorization", &self.access_token)
    }
}
