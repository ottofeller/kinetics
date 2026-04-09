use crate::api::client::Client;
use crate::api::domain;
use crate::api::domain::add::DomainStatus;
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

        self.writer.text(&format!(
            "\n{}...\n",
            console::style(format!("Checking domain {}", self.command.domain))
                .green()
                .bold()
        ))?;

        let client = self.api_client().await?;

        let request = domain::status::Request {
            domain: self.command.domain.clone(),
            project: project_name,
        };

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
            DomainStatus::Ready => {
                self.writer.text(&format!(
                    "\n{}\n",
                    console::style("Domain is ready! You can now deploy with it.")
                        .green()
                        .bold()
                ))?;
            }
            DomainStatus::Error => {
                self.writer.text(&format!(
                    "\n{}\n",
                    console::style("DNS propagation timed out. Check your nameserver settings and try again with `kinetics domain remove` then `kinetics domain add`.")
                        .red()
                        .bold()
                ))?;
            }
            _ => {}
        }

        self.writer.json(json!({
            "domain": response.domain_name,
            "status": response.status,
            "last_checked_at": response.last_checked_at,
        }))?;

        Ok(())
    }
}

fn format_status(response: &domain::status::Response) -> String {
    let status_style = match response.status {
        DomainStatus::Ready => console::style(response.status.to_string()).green(),
        DomainStatus::Error => console::style(response.status.to_string()).red(),
        _ => console::style(response.status.to_string()).yellow(),
    };

    let last_checked = match response.last_checked_at {
        Some(t) => t.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => "Not checked yet".to_string(),
    };

    format!(
        "\n  Domain:       {}\n  Status:       {}\n  Last checked: {}\n",
        console::style(&response.domain_name).bold(),
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
