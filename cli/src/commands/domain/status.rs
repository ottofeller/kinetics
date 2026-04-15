use crate::api::domain;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::WrapErr;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct StatusCommand;

impl Runnable for StatusCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        StatusRunner { writer }
    }
}

struct StatusRunner<'a> {
    writer: &'a Writer,
}

impl Runner for StatusRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;
        let client = self.api_client().await?;

        let domain_name = project
            .domain_name
            .as_ref()
            .ok_or_else(|| {
                self.error(
                    Some("No domain configured"),
                    Some("Add a domain first with `kinetics domain add <domain>`"),
                    None,
                )
            })?
            .clone();

        let request = domain::status::Request {
            domain: domain_name,
            project: project.name,
        };

        let response = client
            .request::<_, domain::status::Response>("/domain/status", request)
            .await
            .wrap_err("Failed to get domain status")
            .map_err(|e| self.server_error(Some(e.into())))?;

        if self.writer.is_structured() {
            self.writer.json(json!({
                "domain": response.domain,
                "status": response.status,
                "last_checked_at": response.last_checked_at,
            }))?;

            return Ok(());
        }

        if response.status.is_pending() {
            self.writer.text(&format!(
                "\n{}\n{}\n",
                console::style("Waiting for DNS propagation")
                    .yellow()
                    .bold(),
                console::style(
                    "Make sure nameservers are set to ns1-4.kineticscloud.com at your registrar"
                )
                .dim()
            ))?;
        }

        if response.status.is_ready() {
            self.writer.text(&format!(
                "\n{}\n{}\n",
                console::style("Nameservers verified").green().bold(),
                console::style("Run `kinetics deploy` to activate the domain").dim()
            ))?;
        }

        if response.status.is_deployed() {
            self.writer.text(&format!(
                "\n{}\n",
                console::style("Domain is live and serving traffic")
                    .green()
                    .bold()
            ))?;
        }

        if response.status.is_error() {
            self.writer.text(&format!(
                "\n{}\n{}\n",
                console::style("DNS verification failed").red().bold(),
                console::style("Check nameservers at your registrar").dim()
            ))?;
        }

        if response.status.is_deleting() {
            self.writer.text(&format!(
                "\n{}\n{}\n",
                console::style("Domain is being removed").yellow().bold(),
                console::style("Run `kinetics deploy` to complete the removal").dim()
            ))?;
        }

        Ok(())
    }
}
