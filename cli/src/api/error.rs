use eyre::WrapErr;
use reqwest::{Response, StatusCode};
use serde::Deserialize;
use serde_json::Value;

/// An HTTP failure with the backend's public error details.
#[derive(Clone, Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: Option<String>,
    pub message: String,
    pub data: Option<Value>,
}

#[derive(Deserialize)]
struct ErrorBody {
    // Legacy endpoints may only return an `error` message.
    code: Option<String>,
    error: String,
    data: Option<Value>,
}

impl ApiError {
    pub async fn from_response(response: Response) -> eyre::Result<Self> {
        let status = response.status();
        let body = response
            .bytes()
            .await
            .wrap_err_with(|| format!("Failed to read backend error response (HTTP {status})"))?;

        let public = serde_json::from_slice::<ErrorBody>(&body).unwrap_or_else(|error| {
            log::debug!("Invalid backend error response (HTTP {status}): {error}");
            ErrorBody {
                code: None,
                error: format!("Backend request failed (HTTP {status}). Try again later."),
                data: None,
            }
        });

        Ok(Self {
            status,
            code: public.code,
            message: public.error,
            data: public.data,
        })
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}
