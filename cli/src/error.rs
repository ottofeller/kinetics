/// Display global error message in unified format
#[derive(Clone, Debug)]
pub struct Error(
    String,
    Option<String>,
    Option<crate::api::client::CompatibilityError>,
);

impl Error {
    pub fn new(message: &str, details: Option<&str>) -> Self {
        Error(message.to_string(), details.map(|d| d.to_string()), None)
    }

    pub(crate) fn from_compatibility(
        mut error: &(dyn std::error::Error + 'static),
    ) -> Option<Self> {
        loop {
            if let Some(compatibility) =
                error.downcast_ref::<crate::api::client::CompatibilityError>()
            {
                return Some(Self(
                    compatibility.to_string(),
                    None,
                    Some(compatibility.clone()),
                ));
            }
            error = error.source()?;
        }
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
        self.2.as_ref().map(|error| error as _)
    }
}

/// Automatically convert all eyre error reports
impl From<eyre::ErrReport> for Error {
    fn from(error: eyre::ErrReport) -> Self {
        if let Some(compatibility) = error.chain().find_map(Self::from_compatibility) {
            return compatibility;
        }
        error
            .downcast::<Error>()
            .unwrap_or_else(|err| Error::new(&err.to_string(), None))
    }
}
