use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use kinetics_api::orgs::owners::add::{Request, Response};
use kinetics_api::request::Validate;
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
            "\n{} {} {} owner {} {org}...\n",
            console::style("Adding").bold(),
            console::style(&username).underlined(),
            console::style("as an").dim(),
            console::style("of").dim(),
        ))?;

        let request = Request {
            org: org.to_owned(),
            username: username.to_owned(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))));
        }

        client
            .request::<Request, Response>("/orgs/owners/add", request)
            .await?;

        self.writer
            .text(&format!("\n{}\n", console::style("Done").bold()))?;

        self.writer
            .json(json!({"success": true, "org": org, "email": username}))?;

        Ok(())
    }
}
