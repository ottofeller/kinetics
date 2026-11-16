use crate::api::error::ApiError;
use crate::credentials::Credentials;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;

use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct LogoutCommand {}

impl Runnable for LogoutCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        LogoutRunner { writer }
    }
}

struct LogoutRunner<'a> {
    writer: &'a Writer,
}

impl Runner for LogoutRunner<'_> {
    /// Logs user out
    ///
    /// By cleaning up the local credentials file and voiding credentials on the backend
    async fn run(&mut self) -> Result<(), Error> {
        let credentials = Credentials::new().await?;

        if credentials.is_valid() {
            self.remove().await?;
        }

        credentials.delete()?;

        self.writer.text(&format!(
            "{} logged out\n",
            console::style("Successfully").bold()
        ))?;

        self.writer.json(json!({ "success": true }))?;
        Ok(())
    }
}

impl LogoutRunner<'_> {
    async fn remove(&mut self) -> Result<(), Error> {
        let client = self.api_client().await?;

        let response = client.post("/auth/logout").send().await?;

        if response.status().is_success() {
            return Ok(());
        }

        let error = ApiError::from_response(response).await?;

        // Ignore invalid_session error, as it means the session is already invalidated
        if error.code == "invalid_session" {
            return Ok(());
        }

        Err(error.into())
    }
}
