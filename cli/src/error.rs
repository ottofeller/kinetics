use crate::api::error::ApiError;
use serde::Serialize;
use serde_json::Value;

/// An error shown to the user, either generated locally or returned by the backend.
#[derive(Clone, Serialize)]
#[serde(untagged)]
pub enum Error {
    Cli {
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },
    Api(ApiError),
}

impl Error {
    pub fn new(message: &str, details: Option<&str>) -> Self {
        Self::Cli {
            code: None,
            message: message.to_owned(),
            details: details.map(str::to_owned),
            data: None,
        }
    }

    pub fn with_code(mut self, value: &str) -> Self {
        match &mut self {
            Self::Cli { code, .. } => *code = Some(value.to_owned()),
            Self::Api(error) => error.code = value.to_owned(),
        }
        self
    }

    pub fn stack_operation(
        response: &kinetics_api::stack::status::Response,
        message: &str,
    ) -> Self {
        let details = response
            .error_details
            .as_ref()
            .filter(|errors| !errors.is_empty())
            .map(|errors| {
                errors
                    .iter()
                    .map(|error| match &error.details {
                        Some(details) => format!("{}\n{details}", error.message),
                        None => error.message.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n")
            })
            .or_else(|| {
                response
                    .errors
                    .as_ref()
                    .filter(|errors| !errors.is_empty())
                    .map(|errors| errors.join("\n"))
            })
            .unwrap_or_else(|| "The operation did not complete. Try again later.".to_owned());
        let code = response
            .error_details
            .as_deref()
            .filter(|errors| errors.len() == 1)
            .map(|errors| errors[0].code.as_str())
            .unwrap_or("stack_operation_failed");
        Self::new(message, Some(&details))
            .with_code(code)
            .with_data(serde_json::json!({
                "status": response.status,
                "errors": response.error_details,
            }))
    }

    pub fn with_data(mut self, value: Value) -> Self {
        match &mut self {
            Self::Cli { data, .. } => *data = Some(value),
            Self::Api(error) => error.data = Some(value),
        }
        self
    }

    pub fn to_json(&self) -> eyre::Result<Value> {
        Ok(serde_json::to_value(self)?)
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(error) => error.fmt(f),
            Self::Cli {
                code,
                message,
                details,
                data,
            } => f
                .debug_struct("Cli")
                .field("code", code)
                .field("message", message)
                .field("details", details)
                .field("data", &data.as_ref().map(|_| "[redacted]"))
                .finish(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(error) => error.fmt(f),
            Self::Cli {
                message, details, ..
            } => {
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
        if let Some(error) = error.downcast_ref::<ApiError>() {
            return Self::Api(error.clone());
        }
        error
            .downcast::<Self>()
            .unwrap_or_else(|error| Self::new(&error.to_string(), None))
    }
}
