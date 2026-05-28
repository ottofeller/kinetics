use crate::api::domains::delete::Request;
use crate::api::request::Validate;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context as _;
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, clap::Args, Clone)]
pub(crate) struct RemoveCommand {
    /// Relative path to the project directory
    #[arg(long)]
    project: Option<PathBuf>,
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
        let project = self.project(&self.command.project).await?;
        let client = self.api_client().await?;
        let domain_name = project.domain_name.clone().ok_or_else(|| {
            Error::new(
                "No domain configured",
                Some("kinetics.toml does not contain a domain entry."),
            )
        })?;

        let request = Request {
            project_name: project.name.clone(),
            domain_name: domain_name.clone(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))));
        }

        if !self.writer.confirm(&format!(
            "Remove domain {} from project {}?",
            domain_name, project.name
        ))? {
            self.writer
                .text(&format!("{}\n", console::style("Canceled").dim().bold()))?;
            return Ok(());
        }

        self.writer.text(&format!(
            "\n{} {}...\n\n",
            console::style("Removing domain").bold().green(),
            console::style(&domain_name).bold(),
        ))?;

        let response = client
            .post("/domains/delete")
            .json(&request)
            .send()
            .await
            .wrap_err("Failed to call /domains/delete endpoint")
            .map_err(|e| self.server_error(Some(e.into())))?;

        if !response.status().is_success() {
            log::error!(
                "Failed to remove domain ({}): {}",
                response.status(),
                response.text().await.unwrap_or("Unknown error".to_string()),
            );

            return Err(self.server_error(None));
        }

        super::config::remove_domain(&project)
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        self.writer.text(&format!(
            "{} {}\n",
            console::style("Removed domain").bold().green(),
            console::style(&domain_name).bold(),
        ))?;

        self.writer.json(json!({
            "success": true,
            "domain": domain_name,
        }))?;

        Ok(())
    }
}
