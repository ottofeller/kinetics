use crate::error::Error;
use crate::function::Function;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use chrono::{DateTime, Utc};
use eyre::Context;
use kinetics_api::func;
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;

#[derive(clap::Args, Clone)]
pub(crate) struct LogsCommand {
    /// Function name to retrieve logs for
    #[arg()]
    name: String,

    /// Time period to get logs for.
    ///
    /// The period (e.g. `1day 3hours`) is a concatenation of time spans.
    /// Where each time span is an integer number and a suffix representing time units.
    ///
    /// Maximum available period is 1 month.
    /// Defaults to 1hour.
    ///
    #[arg(short, long)]
    period: Option<String>,

    /// Relative path to the project directory
    #[arg(long)]
    project: Option<PathBuf>,
}

impl Runnable for LogsCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        LogsRunner {
            command: self.clone(),
            writer,
            interactive: !writer.is_structured() && console::user_attended(),
        }
    }
}

struct LogsRunner<'a> {
    command: LogsCommand,
    writer: &'a Writer,
    /// The terminal is human attended and might require temp notices.
    interactive: bool,
}

impl Runner for LogsRunner<'_> {
    /// Retrieves and displays logs for a specific function
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project(&self.command.project).await?;

        // Get all function names without any additional manipulations.
        let all_functions = project
            .functions()
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        let function = Function::find_by_name(&all_functions, &self.command.name).map_err(|e| {
            self.error(
                Some("Could not find requested function"),
                None,
                Some(e.into()),
            )
        })?;

        let client = self.api_client().await?;

        self.writer.text(&format!(
            "\n{} logs {} {}...\n\n",
            console::style("Fetching").bold(),
            console::style("for").dim(),
            console::style(&function.name)
        ))?;

        let mut events = Vec::new();
        let mut response_period = None;
        let mut cursor = None;
        let mut page = 1usize;

        loop {
            let request = func::logs::Request {
                project: (&project).into(),
                function_name: function.name.clone(),
                period: self.command.period.to_owned(),
                cursor,
            };

            let response = client
                .post("/function/logs")
                .json(&request)
                .send()
                .await
                .wrap_err("Failed to send request to logs endpoint")
                .map_err(|e| self.server_error(Some(e.into())))?;

            // The notice printed for the previous page is erased once this page arrives.
            if self.interactive && page > 1 {
                self.writer.text("\r\x1B[K")?;
            }

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or("Unknown error".to_string());
                log::error!("Failed to fetch logs from API ({}): {}", status, error_text);
                return Err(self.server_error(None));
            }

            let response: func::logs::Response = response
                .json()
                .await
                .wrap_err("Invalid response from server")
                .map_err(|e| self.error(None, None, Some(e.into())))?;

            // Print the period once.
            if response_period.is_none() && !response.events.is_empty() {
                self.writer.text(&format!(
                    "{} {}\n",
                    console::style("Period:").bold(),
                    response.period
                ))?;
            }
            response_period.get_or_insert(response.period);

            for event in response.events {
                // Convert timestamp to readable format
                let datetime = match DateTime::<Utc>::from_timestamp_millis(event.timestamp) {
                    Some(dt) => dt,
                    None => {
                        log::warn!("Invalid timestamp: {}", event.timestamp);
                        continue;
                    }
                };

                let formatted_time = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
                let line = format!("{} {}", console::style(formatted_time).dim(), event.message);
                self.writer.text(&line)?;
                events.push(line);
            }

            if let Some(next_timestamp) = response.next_page {
                cursor = Some(next_timestamp);
                page += 1;

                // Provide fetching notice for human attended terminals.
                if self.interactive {
                    self.writer.text(&format!(
                        "{} {} {}...",
                        console::style("Fetching").bold(),
                        console::style("page").dim(),
                        page,
                    ))?;

                    // No newline follows, so the notice must be flushed
                    // to be visible while the page is loading.
                    std::io::stdout()
                        .flush()
                        .wrap_err("Failed to show the fetching notice")?;
                }
            } else {
                break;
            }
        }

        let period = response_period.unwrap_or_default();

        if events.is_empty() {
            self.writer.text(&format!(
                "{}\n",
                console::style(format!(
                    "No logs found for this function in the last {}.",
                    period
                ))
                .yellow(),
            ))?;
        }

        self.writer
            .json(json!({"success": true, "logs": events, "period": period}))?;

        Ok(())
    }
}
