use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use kinetics_api::orgs::members::invite::{Request, Response};
use kinetics_api::request::Validate;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct InviteMemberCommand {
    /// Email of the person to invite
    email: String,

    /// Name of the org to invite the person to
    #[arg(long)]
    org: String,
}

impl Runnable for InviteMemberCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        InviteMemberRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct InviteMemberRunner<'a> {
    command: InviteMemberCommand,
    writer: &'a Writer,
}

impl Runner for InviteMemberRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let org = self.command.org.clone();
        let email = self.command.email.clone();
        let client = self.api_client().await?;

        self.writer.text(&format!(
            "\n{} {} {} {org}...\n\n",
            console::style("Inviting").bold(),
            console::style(&email).underlined(),
            console::style("to").dim()
        ))?;

        let request = Request {
            org: org.to_owned(),
            email: email.to_owned(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))));
        }

        client
            .request::<Request, Response>("/orgs/members/invite", request)
            .await?;

        self.writer.text(&format!(
            "Invitation has been sent. Please ask the person to check email.\n\n{}\n",
            console::style("Done").bold()
        ))?;

        self.writer
            .json(json!({"success": true, "org": org, "email": email}))?;

        Ok(())
    }
}
