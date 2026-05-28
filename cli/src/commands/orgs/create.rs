use crate::api::orgs::create::{Request, Response};
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
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
        let client = self.api_client().await?;

        self.writer.text(&format!(
            "{}...\n",
            console::style("Creating new org").bold().green()
        ))?;

        let response: Response = client
            .request(
                "/orgs/create",
                Request {
                    name: name.to_owned(),
                },
            )
            .await
            .map_err(|e| self.server_error(Some(e.into())))?;

        self.writer
            .text(&format!("{}\n", console::style("Done").green()))?;

        self.writer
            .json(json!({"success": true, "org": {"id": response.id, "name": name}}))?;

        Ok(())
    }
}
