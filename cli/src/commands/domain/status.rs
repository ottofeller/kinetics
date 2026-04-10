use crate::api::client::Client;
use crate::api::domain;
use crate::api::domain::DomainStatus;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;
use serde_json::json;
use std::time::Duration;

const WATCH_INTERVAL: Duration = Duration::from_secs(30);

#[derive(clap::Args, Clone)]
pub(crate) struct StatusCommand {
    /// Domain name (e.g. example.com)
    #[arg()]
    domain: String,

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
        let project_name = self.project().await?.name;
        let client = self.api_client().await?;

        let request = domain::status::Request {
            domain: self.command.domain.clone(),
            project: project_name,
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

            if !is_watching || response.status != DomainStatus::Pending {
                break;
            }

            self.writer.text(&waiting_msg)?;
            first = false;
            tokio::time::sleep(WATCH_INTERVAL).await;
        }

        match response.status {
            DomainStatus::Pending => {
                self.writer.text(&format!(
                        "\n{}\n{}\n",
                        console::style("Waiting for DNS propagation").yellow().bold(),
                        console::style("Make sure your domain's nameservers are set to ns1-4.kineticscloud.com at your registrar.\
                        ").dim()
                    ))?;
            }
            DomainStatus::Ready => {
                self.writer.text(&format!(
                    "\n{}\n{}\n",
                    console::style("Domain is ready!").green().bold(),
                    console::style("Run `kinetics deploy` to activate it").dim()
                ))?;
            }
            DomainStatus::Deployed => {
                self.writer.text(&format!(
                    "\n{}\n",
                    console::style("Domain is deployed and serving traffic")
                        .green()
                        .bold()
                ))?;
            }
            DomainStatus::Error => {
                self.writer.text(&format!(
                        "\n{}\n{}\n",
                        console::style("DNS propagation timed out").red().bold(),
                        console::style("Verify nameservers at your registrar, then run `kinetics domain remove` and `kinetics domain add` to retry").dim()
                    ))?;
            }
            DomainStatus::Deleting => {
                self.writer.text(&format!(
                    "\n{}\n{}\n",
                    console::style("Domain is being removed").yellow().bold(),
                    console::style("Run `kinetics deploy` to complete the removal").dim()
                ))?;
            }
        }

        Ok(())
    }
}

fn format_status(response: &domain::status::Response) -> String {
    let status_style = match response.status {
        DomainStatus::Ready | DomainStatus::Deployed => {
            console::style(response.status.to_string()).green()
        }
        DomainStatus::Error => console::style(response.status.to_string()).red(),
        _ => console::style(response.status.to_string()).yellow(),
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
