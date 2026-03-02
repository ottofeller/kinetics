use crate::commands::cicd::github;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct InitCommand {
    /// Create a GitHub workflow file.
    #[arg(short, long, action = clap::ArgAction::SetTrue, required = false)]
    github: bool,
}

impl Runnable for InitCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        InitRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct InitRunner<'a> {
    command: InitCommand,
    writer: &'a Writer,
}

impl Runner for InitRunner<'_> {
    /// Initialize a deployment workflow within an existing kinetics project
    ///
    /// Currently only supports GitHub, but expected to expand in the future.
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        self.writer.text(&format!(
            "{}\n",
            console::style("Creating GitHub workflow...").bold().green()
        ))?;

        if self.command.github {
            github::workflow(&project, false, self.writer)
                .map_err(|e| self.error(None, None, Some(e.into())))?;
        }

        self.writer
            .text(&format!("{}\n", console::style("Done").bold().green()))?;

        self.writer.json(json!({"success": true}))?;
        Ok(())
    }
}
