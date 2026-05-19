use crate::api::orgs::owners::add::{Request, Response};
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct AddOwnerCommand {
    /// Username of the person to add as an owner
    username: String,

    /// Name of the org to add the owner to
    #[arg(long)]
    org: String,
}

impl Runnable for AddOwnerCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        AddOwnerRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct AddOwnerRunner<'a> {
    command: AddOwnerCommand,
    writer: &'a Writer,
}

impl Runner for AddOwnerRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let org = self.command.org.clone();
        let username = self.command.username.clone();
        let client = self.api_client().await?;

        self.writer.text(&format!(
            "\n{} {} {} {}...\n",
            console::style("Adding").bold().green(),
            console::style(&username).bold(),
            console::style("as an owner of").bold().green(),
            console::style(&org).bold()
        ))?;

        client
            .request::<Request, Response>(
                "/orgs/owners/add",
                Request {
                    org: org.to_owned(),
                    username: username.to_owned(),
                },
            )
            .await?;

        self.writer
            .text(&format!("\n{}\n", console::style("Done").green().bold()))?;

        self.writer
            .json(json!({"success": true, "org": org, "email": username}))?;

        Ok(())
    }
}
