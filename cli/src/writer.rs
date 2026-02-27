use crate::error::Error;
use std::io::{Stderr, Stdout, Write};

/// Write all stdout/stderr outputs in the app
///
/// In either plain text mode or structured (e.g. JSON).
#[derive(Default)]
pub(crate) struct Writer {
    is_structured: bool,
}

/// Implement default for ref
///
/// Some structs with Default trait are storing a writer as a ref.
impl<'a> Default for &'a Writer {
    fn default() -> &'a Writer {
        static WRITER: Writer = Writer {
            is_structured: false,
        };

        &WRITER
    }
}

impl Writer {
    pub(crate) fn new(is_structured: bool) -> Self {
        Writer { is_structured }
    }

    /// Output plain text
    ///
    /// Prints out nothing but a warning (in warn log level) when the writer is in structured mode.
    pub(crate) fn text(&self, output: &str) -> Result<(), Error> {
        if self.is_structured {
            log::warn!("Skipping output (not structured data): {output}");
            return Ok(());
        }

        self.write(output, false)?;
        Ok(())
    }

    /// Output serialized JSON
    ///
    /// Prints out nothing but a warning (in warn log level) when the writer is in plain text mode.
    pub(crate) fn json(&self, output: serde_json::Value) -> Result<(), Error> {
        if !self.is_structured {
            log::warn!("Skipping output (not plain text): {output}");
            return Ok(());
        }

        self.write(&format!("{}\n", &output.to_string()), false)?;
        Ok(())
    }

    /// Output plain text in stderr
    ///
    /// Prints out nothing but a warning (in warn log level) when the writer is in structured mode.
    pub(crate) fn error(&self, output: &str) -> Result<(), Error> {
        if self.is_structured {
            log::warn!("Skipping output (not structured data): {output}");
            return Ok(());
        }

        self.write(output, true)
    }

    /// General method for writing to stdout/stderr
    fn write(&self, output: &str, is_error: bool) -> Result<(), Error> {
        let mut stderr: Stderr = std::io::stderr();
        let mut stdout: Stdout = std::io::stdout();
        let stream: &mut dyn Write = if is_error { &mut stderr } else { &mut stdout };

        stream.write(output.as_bytes()).map_err(|e| {
            log::error!("Error while writing to std*: {e:?}");

            Error::new(
                "Output error",
                Some("Please report a bug at support@deploykinetics.com"),
            )
        })?;

        Ok(())
    }

    pub(crate) fn is_structured(&self) -> bool {
        self.is_structured
    }
}
