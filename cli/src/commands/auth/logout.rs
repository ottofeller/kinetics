use crate::api::auth;
use crate::credentials::Credentials;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;
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
            self.remove(&credentials.email)
                .await
                .wrap_err("Logout request failed")?;
        }

        credentials.delete()?;

        self.writer.text(&format!(
            "{}\n",
            console::style("Successfully logged out").green().bold()
        ))?;

        self.writer.json(json!({ "success": true }))?;
        Ok(())
    }
}

impl LogoutRunner<'_> {
    async fn remove(&mut self, email: &str) -> eyre::Result<()> {
        let client = self.api_client().await?;

        let response = client
            .post("/auth/logout")
            .json(&auth::logout::Request {
                email: email.to_owned(),
            })
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(eyre::eyre!("Failed to logout: {}", response.text().await?));
        }

        Ok(())
    }
}
