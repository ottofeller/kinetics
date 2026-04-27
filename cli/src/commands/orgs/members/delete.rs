use crate::api::orgs::members::delete::{Request, Response};
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use crossterm::style::Stylize;
use serde_json::json;
use std::io::{stdin, stdout, Write};

#[derive(clap::Args, Clone)]
pub(crate) struct DeleteMemberCommand {
    /// Username of the member to remove
    username: String,

    /// Name of the org to remove the member from
    #[arg(long)]
    org: String,
}

impl Runnable for DeleteMemberCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        DeleteMemberRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct DeleteMemberRunner<'a> {
    command: DeleteMemberCommand,
    writer: &'a Writer,
}

impl Runner for DeleteMemberRunner<'_> {
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
                "\nAre you sure you want to remove {} from {}? {} ",
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
            console::style("Removing member").bold().green()
        ))?;

        client
            .request::<Request, Response>(
                "/orgs/members/delete",
                Request {
                    org: org.clone(),
                    username: username.clone(),
                },
            )
            .await
            .map_err(|e| self.server_error(Some(e.into())))?;

        self.writer
            .text(&format!("\n{}\n", console::style("Done").green().bold()))?;

        self.writer
            .json(json!({"success": true, "org": org, "username": username}))?;

        Ok(())
    }
}
