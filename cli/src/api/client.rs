use super::error::ApiError;
use crate::config::api_url;
use crate::credentials::Credentials;
use crate::error::Error;
use chrono::Utc;
use eyre::WrapErr;
use kinetics_api::compatibility::{MIN_VERSION_HEADER, VERSION_HEADER};
use kinetics_api::request::to_log_safe_string_pretty;
use reqwest::{Method, StatusCode};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::OnceCell;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const UPGRADE_COMMAND: &str = "cargo install kinetics --locked --force";
static WARNED: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
pub struct Client {
    access_token: String,
    client: reqwest::Client,
}

pub struct RequestBuilder(reqwest::RequestBuilder);

impl RequestBuilder {
    pub fn json<T: Serialize + ?Sized>(self, body: &T) -> Self {
        Self(self.0.json(body))
    }

    pub async fn send(self) -> Result<reqwest::Response, Error> {
        let response = self
            .0
            .header(VERSION_HEADER, VERSION)
            .send()
            .await
            .wrap_err(Error::new(
                "Network request failed",
                Some("Try again in a few seconds."),
            ))?;
        let minimum = response
            .headers()
            .get(MIN_VERSION_HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| Version::parse(value).ok());

        if response.status() == StatusCode::UPGRADE_REQUIRED {
            return Err(ApiError::from_response(response).await?.into());
        }

        // The backend advertises its minimum even when enforcement is disabled.
        // In that mode, warn once and let the request continue.
        if let Some(minimum) = minimum {
            let current = Version::parse(VERSION).wrap_err("Invalid CLI version")?;
            if current.cmp_precedence(&minimum).is_lt() && !WARNED.swap(true, Ordering::Relaxed) {
                eprintln!(
                    "{}: kinetics {VERSION} is below the backend minimum {minimum}.\n\
                     Update with: {UPGRADE_COMMAND}",
                    console::style("Warning").yellow()
                );
            }
        }

        Ok(response)
    }
}

static CREDENTIALS: OnceCell<Credentials> = OnceCell::const_new();

impl Client {
    pub async fn new(is_directly: bool) -> Result<Self, Error> {
        if is_directly {
            return Ok(Self::unauthenticated());
        }

        let credentials = CREDENTIALS
            .get_or_try_init(async || Credentials::new().await)
            .await?;

        // If credentials expired — request to re-login
        if credentials.expires_at.timestamp() <= Utc::now().timestamp() {
            return Err(Error::new("Credentials expired, please re-login.", None));
        }

        Ok(Self::with_token(&credentials.token))
    }

    pub(crate) fn unauthenticated() -> Self {
        Self::with_token("")
    }

    pub(crate) fn with_token(token: &str) -> Self {
        Self {
            access_token: token.to_owned(),
            client: reqwest::Client::new(),
        }
    }

    /// A POST request with the Authorization header
    pub fn post(&self, path: &str) -> RequestBuilder {
        self.builder(Method::POST, path)
    }

    pub(crate) fn get(&self, path: &str) -> RequestBuilder {
        self.builder(Method::GET, path)
    }

    fn builder(&self, method: Method, path: &str) -> RequestBuilder {
        let mut request = self.client.request(method, api_url(path));
        if !self.access_token.is_empty() {
            request = request.header("Authorization", &self.access_token);
        }
        RequestBuilder(request)
    }

    /// Encapsulate a typical POST request
    pub async fn request<B, R>(&self, path: &str, body: B) -> Result<R, Error>
    where
        B: Serialize + for<'de> Deserialize<'de>,
        R: Serialize + for<'de> Deserialize<'de>,
    {
        let result = self
            .post(path)
            .json(&body)
            .send()
            .await
            .inspect_err(|err| log::error!("{err:?}"))?;

        let status = result.status();
        log::info!("Got status from {path}: {status}");

        if status != StatusCode::OK {
            return Err(ApiError::from_response(result).await?.into());
        }

        let response = result
            .json::<R>()
            .await
            .inspect_err(|error| log::debug!("Invalid response from {path}: {error:?}"))
            .wrap_err(Error::new("Server error", Some("Try again later.")))?;
        log::info!(
            "Got response from {path}: {}",
            to_log_safe_string_pretty(&response)
                .unwrap_or_else(|_| "[unavailable response]".to_owned())
        );

        Ok(response)
    }
}
