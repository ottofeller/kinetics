use crate::config::build_config;
use crate::credentials::Credentials;
use chrono::Utc;
use std::path::Path;

#[derive(Clone)]
pub struct Client {
    access_token: String,
    client: reqwest::Client,
}

impl Client {
    pub fn new(is_directly: bool) -> eyre::Result<Self> {
        if is_directly {
            return Ok(Client {
                access_token: "".into(),
                client: reqwest::Client::new(),
            });
        }

        let credentials = Credentials::new(&Path::new(&build_config()?.credentials_path))?;

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
}
