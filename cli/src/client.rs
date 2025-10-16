use crate::config::build_config;
use crate::credentials::Credentials;
use crate::error::Error;
use chrono::Utc;
use eyre::{Ok, WrapErr};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Client {
    access_token: String,
    client: reqwest::Client,
}

impl Client {
    pub async fn new(is_directly: bool) -> eyre::Result<Self> {
        if is_directly {
            return Ok(Client {
                access_token: "".into(),
                client: reqwest::Client::new(),
            });
        }

        let credentials = Credentials::new().await?;

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
        format!("{}{}", build_config().unwrap().api_base, path)
    }

    /// A POST request with the Authorization header
    pub fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .post(Self::url(path))
            .header("Authorization", &self.access_token)
    }

    /// Incapsulate a typical POST request
    pub async fn request<B, R>(&self, path: &str, body: B) -> eyre::Result<R>
    where
        B: Serialize + for<'de> Deserialize<'de>,
        R: Serialize + for<'de> Deserialize<'de>,
    {
        let result = self
            .post(path)
            .json(&body)
            .send()
            .await
            .inspect_err(|err| log::error!("{err:?}"))
            .wrap_err(Error::new(
                "Network request failed",
                Some("Try again in a few seconds."),
            ))?;

        let status = result.status();
        let text = result.text().await?;
        log::info!("Got status from {path}: {status}");
        log::info!("Got response from {path}: {text}");

        if status != StatusCode::OK {
            return Err(Error::new("Request failed", Some("Try again in a few seconds.")).into());
        }

        Ok(serde_json::from_str(&text).wrap_err("Could not parse")?)
    }
}
