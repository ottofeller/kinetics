use crate::api::orgs::owners::delete::{Request, Response};
use crate::api::request::Validate;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use crossterm::style::Stylize;
use serde_json::json;
use std::io::{stdin, stdout, Write};

#[derive(clap::Args, Clone)]
pub(crate) struct DeleteOwnerCommand {
    /// Username of the owner to demote
    username: String,

    /// Name of the org to demote the owner from
    #[arg(long)]
    org: String,
}

impl Runnable for DeleteOwnerCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        DeleteOwnerRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct DeleteOwnerRunner<'a> {
    command: DeleteOwnerCommand,
    writer: &'a Writer,
}

impl Runner for DeleteOwnerRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let client = self.api_client().await?;

        let generic_error = Error::new(
            "Failed to process the command",
            Some("Please report a bug at support@deploykinetics.com"),
        );

        let org = self.command.org.clone();
        let username = self.command.username.clone();

        // Ask for confirmation (skip in structured/JSON mode)
        if !self.writer.is_structured() {
            self.writer.text(&format!(
                "\nAre you sure you want to demote {} from being an owner of {}? {} ",
                username.clone().white().bold(),
                org.clone().white().bold(),
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
            console::style("Demoting owner").bold().green()
        ))?;

        let request = Request {
            org: org.clone(),
            username: username.clone(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))).into());
        }

        client
            .request::<Request, Response>("/orgs/owners/delete", request)
            .await?;

        self.writer
            .text(&format!("\n{}\n", console::style("Done").green().bold()))?;

        self.writer
            .json(json!({"success": true, "org": org, "username": username}))?;

        Ok(())
    }
}
