use crate::api::{client::CompatibilityError, error::ApiError};
use serde_json::{json, Value};

/// Display global error message in unified format
#[derive(Clone, Debug)]
pub struct Error(String, Option<String>, Option<Source>);

#[derive(Clone, Debug)]
enum Source {
    Compatibility(CompatibilityError),
    Api(Box<ApiError>),
}

impl Error {
    pub fn new(message: &str, details: Option<&str>) -> Self {
        Error(message.to_string(), details.map(|d| d.to_string()), None)
    }

    pub(crate) fn from_compatibility(
        mut error: &(dyn std::error::Error + 'static),
    ) -> Option<Self> {
        loop {
            if let Some(compatibility) = error.downcast_ref::<CompatibilityError>() {
                return Some(Self(
                    compatibility.to_string(),
                    None,
                    Some(Source::Compatibility(compatibility.clone())),
                ));
            }
            error = error.source()?;
        }
    }

    /// Preserve public errors through command-specific error contexts.
    pub(crate) fn from_source(mut error: &(dyn std::error::Error + 'static)) -> Option<Self> {
        if let Some(error) = Self::from_compatibility(error) {
            return Some(error);
        }

        loop {
            if let Some(api) = error.downcast_ref::<ApiError>() {
                return Some(Self(
                    api.message.clone(),
                    None,
                    Some(Source::Api(Box::new(api.clone()))),
                ));
            }
            error = error.source()?;
        }
    }

    /// Returns structured output without formatting the backend's public message.
    pub fn to_json(&self) -> Value {
        let mut output = json!({"success": false, "error": self.to_string()});

        if let Some(Source::Api(error)) = &self.2 {
            output["error"] = json!(error.message);
            output["status"] = json!(error.status.as_u16());

            if let Some(code) = &error.code {
                output["code"] = json!(code);
            }
            if let Some(data) = &error.data {
                output["data"] = data.clone();
            }
        }

        output
    }
}

/// Display the message and details, as sort of a hint
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}\n\n{}",
            self.0,
            console::style(self.1.clone().unwrap_or("".into())).dim()
        )
    }
}

/// Implement std::error::Error trait for Error
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.2 {
            Some(Source::Compatibility(error)) => Some(error),
            Some(Source::Api(error)) => Some(error.as_ref()),
            None => None,
        }
    }
}

/// Automatically convert all eyre error reports
impl From<eyre::ErrReport> for Error {
    fn from(error: eyre::ErrReport) -> Self {
        if let Some(public) = error.chain().find_map(Self::from_source) {
            return public;
        }
        error
            .downcast::<Error>()
            .unwrap_or_else(|err| Error::new(&err.to_string(), None))
    }
}
