use crate::api::domain;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use crossterm::style::Stylize;
use eyre::Context;
use serde_json::json;
use std::io::{self, Write};

#[derive(clap::Args, Clone)]
pub(crate) struct RemoveCommand {
    /// Domain name (e.g. example.com)
    #[arg()]
    domain: String,
}

impl Runnable for RemoveCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        RemoveRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct RemoveRunner<'a> {
    command: RemoveCommand,
    writer: &'a Writer,
}

impl Runner for RemoveRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let client = self.api_client().await?;
        let project_name = self.project().await?.name;

        // Ask for confirmation (skip in structured/JSON mode)
        if !self.writer.is_structured() {
            self.writer.text(&format!(
                "You are removing domain \"{}\" from project \"{}\".\n",
                self.command.domain.as_str().blue().bold(),
                project_name.as_str().blue().bold(),
            ))?;
            self.writer.text(&format!(
                "{} {}: ",
                "Do you want to proceed?".bold(),
                "[y/N]".dim()
            ))?;

            io::stdout()
                .flush()
                .wrap_err("Failed to process stdout")
                .map_err(|e| self.error(None, None, Some(e.into())))?;

            let mut input = String::new();

            io::stdin()
                .read_line(&mut input)
                .wrap_err("Failed to read user's input")
                .map_err(|e| self.error(None, None, Some(e.into())))?;

            if !matches!(input.trim().to_lowercase().as_ref(), "y" | "yes") {
                self.writer
                    .text(&format!("{}\n", "Domain removal canceled".dim().bold()))?;
                return Ok(());
            }
        }

        self.writer.text(&format!(
            "\n{}...\n",
            console::style(format!("Removing domain {}", self.command.domain))
                .green()
                .bold()
        ))?;

        let response: domain::remove::Response = client
            .request(
                "/domain/remove",
                domain::remove::Request {
                    domain: self.command.domain.clone(),
                    project: project_name,
                },
            )
            .await
            .wrap_err("Failed to remove domain")
            .map_err(|e| self.server_error(Some(e.into())))?;

        let message = if response.requires_deploy {
            "Domain marked for removal. Run `kinetics deploy` to complete the cleanup."
        } else {
            "Domain removal initiated. The hosted zone and DNS records will be cleaned up shortly."
        };

        self.writer
            .text(&format!("\n{}\n", console::style(message).green().bold()))?;

        self.writer.json(json!({
            "domain": response.domain,
            "status": response.status,
        }))?;

        Ok(())
    }
}
