use crate::api::auth::tokens::delete::Request;
use crate::api::request::Validate;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use crossterm::style::Stylize;
use eyre::Context;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct DeleteCommand {
    /// Name of the access token to delete
    name: String,
}

impl Runnable for DeleteCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        DeleteRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct DeleteRunner<'a> {
    command: DeleteCommand,
    writer: &'a Writer,
}

impl Runner for DeleteRunner<'_> {
    /// Deletes an access token
    async fn run(&mut self) -> Result<(), Error> {
        if !self
            .writer
            .confirm(&format!("Delete access token {}?", self.command.name))?
        {
            self.writer.text(&format!("{}\n", "Canceled".yellow()))?;
            return Ok(());
        }

        self.writer.text(&format!(
            "\n{}...\n",
            console::style("Deleting access token").bold().green()
        ))?;

        let client = self.api_client().await?;

        let request = Request {
            name: self.command.name.clone(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))).into());
        }

        let response = client
            .post("/auth/tokens/delete")
            .json(&request)
            .send()
            .await
            .wrap_err("Failed to call token deletion endpoint")?;

        if !response.status().is_success() {
            let response: serde_json::Value = response.json().await.wrap_err(Error::new(
                "Invalid response from server",
                Some("Try again later."),
            ))?;

            return Err(Error::new(
                "Failed to delete token",
                Some(
                    response
                        .get("error")
                        .unwrap_or(&serde_json::Value::Null)
                        .as_str()
                        .unwrap_or("Unknown error"),
                ),
            )
            .into());
        }

        self.writer
            .text(&format!("\n{}\n", console::style("Deleted").green().bold()))?;

        self.writer.json(json!({"success": true}))?;
        Ok(())
    }
}
