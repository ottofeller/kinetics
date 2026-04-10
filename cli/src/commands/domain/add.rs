use crate::api::domain;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct AddCommand {
    /// Domain name (e.g. example.com)
    #[arg()]
    domain: String,
}

impl Runnable for AddCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        AddRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct AddRunner<'a> {
    command: AddCommand,
    writer: &'a Writer,
}

impl Runner for AddRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let project_name = self.project().await?.name;

        self.writer.text(&format!(
            "\n{}...\n",
            console::style(format!("Adding domain {}", self.command.domain))
                .green()
                .bold()
        ))?;

        let client = self.api_client().await?;

        let response: domain::add::Response = client
            .request(
                "/domain/add",
                domain::add::Request {
                    domain: self.command.domain.clone(),
                    project: project_name,
                },
            )
            .await
            .wrap_err("Failed to add domain")
            .map_err(|e| self.server_error(Some(e.into())))?;

        self.writer.text(&format!(
            "\n{}\n\n\
            Update your domain's nameservers at your registrar:\n\n{}\n\n\
            DNS propagation may take up to 48 hours.\n\
            Run {} to check progress.\n\n",
            console::style(format!("Domain {} added successfully.", response.domain))
                .green()
                .bold(),
            response
                .nameservers
                .iter()
                .map(|ns| format!("  {}", console::style(ns).bold()))
                .collect::<Vec<_>>()
                .join("\n"),
            console::style(format!("kinetics domain status {}", response.domain)).cyan(),
        ))?;

        self.writer.json(json!({
            "domain": response.domain,
            "status": response.status,
            "nameservers": response.nameservers,
        }))?;

        Ok(())
    }
}
