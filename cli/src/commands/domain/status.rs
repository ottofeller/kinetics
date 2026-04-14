use crate::api::client::Client;
use crate::api::domain;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;
use serde_json::json;
use std::time::Duration;

const WATCH_INTERVAL: Duration = Duration::from_secs(10);

#[derive(clap::Args, Clone)]
pub(crate) struct StatusCommand {
    /// Poll until the domain is ready or fails
    #[arg(long, default_value_t = false)]
    watch: bool,
}

impl Runnable for StatusCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        StatusRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct StatusRunner<'a> {
    command: StatusCommand,
    writer: &'a Writer,
}

impl Runner for StatusRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;
        let client = self.api_client().await?;

        let domain_name = project
            .domain
            .as_ref()
            .ok_or_else(|| {
                self.error(
                    Some("No domain configured"),
                    Some("Add a domain first with `kinetics domain add <domain>`"),
                    None,
                )
            })?
            .name
            .clone();

        let request = domain::status::Request {
            domain: domain_name,
            project: project.name,
        };

        let response = check_status(&client, &request)
            .await
            .map_err(|e| self.server_error(Some(e.into())))?;

        if self.writer.is_structured() {
            self.writer.json(json!({
                "domain": response.domain,
                "status": response.status,
                "last_checked_at": response.last_checked_at,
            }))?;

            return Ok(());
        }

        let is_watching = self.command.watch;
        let waiting_msg = format!(
            "{}",
            console::style("\nWaiting for DNS propagation... (Ctrl+C to stop)\n").dim()
        );
        let mut first = true;
        let mut response;

        loop {
            response = check_status(&client, &request)
                .await
                .map_err(|e| self.server_error(Some(e.into())))?;

            if !first {
                self.writer.text("\x1b[6A\x1b[0J")?;
            }

            self.writer.text(&format_status(&response))?;

            if !is_watching || response.status.is_pending() {
                break;
            }

            self.writer.text(&waiting_msg)?;
            first = false;
            tokio::time::sleep(WATCH_INTERVAL).await;
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

fn format_status(response: &domain::status::Response) -> String {
    let status_style = if response.status.is_ready() || response.status.is_deployed() {
        console::style(response.status.to_string()).green()
    } else if response.status.is_error() {
        console::style(response.status.to_string()).red()
    } else {
        console::style(response.status.to_string()).yellow()
    };

    let last_checked = match response.last_checked_at {
        Some(t) => t.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => "Not checked yet".to_string(),
    };

    format!(
        "\n  Domain:       {}\n  Status:       {}\n  Last checked: {}\n",
        console::style(&response.domain).bold(),
        status_style.bold(),
        last_checked,
    )
}

async fn check_status(
    client: &Client,
    request: &domain::status::Request,
) -> eyre::Result<domain::status::Response> {
    client
        .request("/domain/status", request.clone())
        .await
        .wrap_err("Failed to get domain status")
}
