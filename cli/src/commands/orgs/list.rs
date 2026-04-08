use crate::api::orgs::list::{Request, Response};
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct ListCommand;

impl Runnable for ListCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        ListRunner { writer }
    }
}

struct ListRunner<'a> {
    writer: &'a Writer,
}

impl Runner for ListRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        self.writer.text(&format!(
            "\n{}...\n\n",
            console::style("Fetching orgs").bold().green()
        ))?;

        let client = self.api_client().await?;

        let response: Response = client
            .request("/orgs/list", Request {})
            .await
            .map_err(|e| self.server_error(Some(e.into())))?;

        if response.orgs.is_empty() {
            self.writer
                .text(&format!("{}", console::style("No orgs found").yellow()))?;

            self.writer.json(json!({"success": true, "orgs": []}))?;
            return Ok(());
        }

        for org in &response.orgs {
            self.writer.text(&format!(
                "{}{}",
                console::style(&org.name).white().bold(),
                if org.is_owner {
                    format!("{}", console::style(" (owner)").dim())
                } else {
                    "".into()
                },
            ))?;

            for member in &org.members {
                self.writer.text(&format!("\n{}", member.email))?;
            }

            self.writer.text("\n\n")?;
        }

        self.writer
            .json(json!({"success": true, "orgs": response.orgs}))?;

        Ok(())
    }
}
