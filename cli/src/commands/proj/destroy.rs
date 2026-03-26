use crate::error::Error;
use crate::project::Project;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use crossterm::style::Stylize;
use eyre::Context;
use serde_json::json;
use std::io::{self, Write};

#[derive(clap::Args, Clone)]
pub(crate) struct DestroyCommand {
    /// Name of the project to destroy (optional, defaults to current project name)
    #[arg(short, long)]
    name: Option<String>,
}

impl Runnable for DestroyCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        DestroyRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct DestroyRunner<'a> {
    command: DestroyCommand,
    writer: &'a Writer,
}

impl Runner for DestroyRunner<'_> {
    /// Destroys a project after user confirmation
    async fn run(&mut self) -> Result<(), Error> {
        let current_project = self.project().await?;

        let project_name = match &self.command.name {
            Some(name) => name.as_str(),
            None => current_project.name.as_str(),
        };

        let project = match Project::fetch_one(project_name).await {
            Ok(project) => project,

            Err(_) => {
                self.writer
                    .text(&format!("{}\n", "Project not found".yellow()))?;

                self.writer
                    .json(json!({"success": false, "message": "Project not found"}))?;

                return Ok(());
            }
        };

        // Ask for confirmation (skip in structured/JSON mode)
        if !self.writer.is_structured() {
            self.writer.text(&format!(
                "You are destroying \"{}\" project.\n",
                project.name.as_str().blue().bold()
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
                    .text(&format!("{}\n", "Destroying canceled".dim().bold()))?;
                return Ok(());
            }
        }

        self.writer
            .text(&format!("{}: {}\n", "Destroying".bold(), &project.name))?;

        project
            .destroy()
            .await
            .wrap_err("Project destroy request failed")
            .map_err(|e| self.server_error(Some(e.into())))?;

        self.writer.text(&format!(
            "{}\n",
            console::style("Project destroyed").green()
        ))?;

        self.writer.json(json!({"success": true}))?;
        Ok(())
    }
}
