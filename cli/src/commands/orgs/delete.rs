use crate::api::orgs::delete::{Request, Response};
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use crossterm::style::Stylize;
use serde_json::json;
use std::io::{stdin, stdout, Write};

#[derive(clap::Args, Clone)]
pub(crate) struct DeleteCommand {
    /// Name of the org to delete
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
    async fn run(&mut self) -> Result<(), Error> {
        let client = self.api_client().await?;

        let generic_error = Error::new(
            "Failed to process the command",
            Some("Please report a bug at support@deploykinetics.com"),
        );

        let name = self.command.name.clone();

        // Ask for confirmation (skip in structured/JSON mode)
        if !self.writer.is_structured() {
            self.writer.text(&format!(
                "\nAre you sure want to delete org {}? {} ",
                name.clone().white().bold(),
                "[y/N]".dim()
            ))?;

            let mut input = String::new();

            stdout().flush().map_err(|e| {
                log::error!("Failed to write to stdout: {e:?}");
                generic_error.clone()
            })?;

            stdin().read_line(&mut input).map_err(|e| {
                log::error!("Failed to read from stdin: {e:?}");
                generic_error
            })?;

            if !matches!(input.trim().to_lowercase().as_ref(), "y" | "yes") {
                self.writer.text(&format!("{}\n", "Canceled".yellow()))?;
                return Ok(());
            }
        }

        self.writer.text(&format!(
            "\n{}...\n",
            console::style("Deleting org").bold().green()
        ))?;

        let response: Response = client
            .request(
                "/orgs/delete",
                Request {
                    name: name.to_owned(),
                },
            )
            .await
            .map_err(|e| self.server_error(Some(e.into())))?;

        self.writer
            .text(&format!("\n{}\n", console::style("Done").green().bold()))?;

        self.writer
            .json(json!({"success": true, "org": {"id": response.id, "name": name}}))?;

        Ok(())
    }
}
