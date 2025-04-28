/// Display global error message in unified format
#[derive(Debug)]
pub struct Error(String, Option<String>);

impl Error {
    pub fn new(message: &str, details: Option<&str>) -> Self {
        Error(message.to_string(), details.map(|d| d.to_string()))
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
impl std::error::Error for Error {}

/// Automatically convert all eyre error reports
impl From<eyre::ErrReport> for Error {
    fn from(error: eyre::ErrReport) -> Self {
        let error = error
            .downcast::<Error>()
            .unwrap_or_else(|err| Error::new(&err.to_string(), None));

        eprintln!("\n\n{}\n{error}", console::style("Error").red().bold());

        // The Error should be used as a terminating error
        std::process::exit(1)
    }
}
