use eyre::WrapErr;
use reqwest::{Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An HTTP failure with the backend's public error details.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiError {
    /// The HTTP status code of the response, serialized as a number.
    #[serde(skip_deserializing, serialize_with = "serialize_status")]
    pub status: StatusCode,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

fn serialize_status<S>(status: &StatusCode, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_u16(status.as_u16())
}

impl ApiError {
    pub async fn from_response(response: Response) -> eyre::Result<Self> {
        let status = response.status();
        let mut public = response
            .json::<Self>()
            .await
            .wrap_err_with(|| format!("Invalid backend error response (HTTP {status})"))?;

        public.status = status;
        Ok(public)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)?;
        if let Some(details) = &self.details {
            write!(f, "\n\n{details}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ApiError {}
