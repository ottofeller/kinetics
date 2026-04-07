use crate::error::Error;
use crate::org::OrgBuilder;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use crossterm::style::Stylize;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct CreateCommand {
    /// Name of the org to create
    name: String,
}

impl Runnable for CreateCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        CreateRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct CreateRunner<'a> {
    command: CreateCommand,
    writer: &'a Writer,
}

impl Runner for CreateRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let name = self.command.name.clone();

        self.writer
            .text(&format!("{}...\n", "Creating new org".bold()))?;

        let org = OrgBuilder::create(&name)
            .await
            .inspect_err(|e| log::error!("Creation of the org failed: {e}"))?;

        self.writer
            .text(&format!("{}\n", console::style("Done").green()))?;

        self.writer.json(json!({"success": true, "org": org}))?;
        Ok(())
    }
}
