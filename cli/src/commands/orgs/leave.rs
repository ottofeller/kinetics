use crate::api::orgs::leave::{Request, Response};
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use crossterm::style::Stylize;
use serde_json::json;
use std::io::{stdin, stdout, Write};

#[derive(clap::Args, Clone)]
pub(crate) struct LeaveCommand {
    /// Name of the org to leave
    #[arg(long)]
    org: String,
}

impl Runnable for LeaveCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        LeaveRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct LeaveRunner<'a> {
    command: LeaveCommand,
    writer: &'a Writer,
}

impl Runner for LeaveRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let client = self.api_client().await?;

        let generic_error = Error::new(
            "Failed to process the command",
            Some("Please report a bug at support@deploykinetics.com"),
        );

        let org = self.command.org.clone();

        // Ask for confirmation (skip in structured/JSON mode)
        if !self.writer.is_structured() {
            self.writer.text(&format!(
                "\nAre you sure you want to leave org {}? {} ",
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
            "\n{} {}...\n",
            console::style("Leaving the org").bold().green(),
            console::style(&org).bold(),
        ))?;

        client
            .request::<Request, Response>(
                "/orgs/leave",
                Request {
                    org: org.to_owned(),
                },
            )
            .await?;

        self.writer
            .text(&format!("\n{}\n", console::style("Done").green().bold()))?;

        self.writer
            .json(json!({"success": true, "org": {"name": org}}))?;

        Ok(())
    }
}
