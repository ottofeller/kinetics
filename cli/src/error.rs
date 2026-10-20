use crate::api::error::ApiError;
use serde::Serialize;
use serde_json::Value;

/// An error shown to the user, either generated locally or returned by the backend.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum Error {
    Cli {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<String>,
    },
    Api(ApiError),
}

impl Error {
    pub fn new(message: &str, details: Option<&str>) -> Self {
        Self::Cli {
            message: message.to_owned(),
            details: details.map(str::to_owned),
        }
    }

    pub fn to_json(&self) -> eyre::Result<Value> {
        Ok(serde_json::to_value(self)?)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(error) => error.fmt(f),
            Self::Cli { message, details } => {
                f.write_str(message)?;
                if let Some(details) = details {
                    write!(f, "\n\n{}", console::style(details).dim())?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Api(error) => Some(error),
            Self::Cli { .. } => None,
        }
    }
}

impl From<ApiError> for Error {
    fn from(error: ApiError) -> Self {
        Self::Api(error)
    }
}

/// Bridge operations that still return technical error reports.
impl From<eyre::Report> for Error {
    fn from(error: eyre::Report) -> Self {
        log::debug!("{error:?}");
        error
            .downcast::<Self>()
            .unwrap_or_else(|error| Self::new(&error.to_string(), None))
    }
}
