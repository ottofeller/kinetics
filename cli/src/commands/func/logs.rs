use crate::api::func;
use crate::error::Error;
use crate::function::Function;
use crate::runner::{Runnable, Runner};
use chrono::{DateTime, Utc};
use eyre::Context;
use kinetics_parser::Parser;

#[derive(clap::Args, Clone)]
pub(crate) struct LogsCommand {
    /// Function name to retrieve logs for
    #[arg()]
    name: String,

    /// Time period to get logs for.
    ///
    /// The period object (e.g. `1day 3hours`) is a concatenation of time spans.
    /// Where each time span is an integer number and a suffix representing time units.
    ///
    /// Maximum available period is 1 month.
    /// Defaults to 1hour.
    ///
    #[arg(short, long)]
    period: Option<String>,
}

impl Runnable for LogsCommand {
    fn runner(&self) -> impl Runner {
        LogsRunner {
            command: self.clone(),
        }
    }
}

struct LogsRunner {
    command: LogsCommand,
}

impl Runner for LogsRunner {
    /// Retrieves and displays logs for a specific function
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        // Get all function names without any additional manipulations.
        let all_functions = Parser::new(Some(&project.path))
            .map_err(|e| self.error(None, None, Some(e.into())))?
            .functions
            .into_iter()
            .map(|f| Function::new(&project, &f))
            .collect::<eyre::Result<Vec<Function>>>()
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        let function = Function::find_by_name(&all_functions, &self.command.name).map_err(|e| {
            self.error(
                Some("Cound not find requested function"),
                None,
                Some(e.into()),
            )
        })?;

        let client = self.api_client().await?;

        println!(
            "\n{} {} {}...\n",
            console::style("Fetching logs").bold().green(),
            console::style("for").dim(),
            console::style(&function.name).bold()
        );

        let response = client
            .post("/function/logs")
            .json(&func::logs::Request {
                project_name: project.name.clone(),
                function_name: function.name.clone(),
                period: self.command.period.to_owned(),
            })
            .send()
            .await
            .wrap_err("Failed to send request to logs endpoint")
            .map_err(|e| self.server_error(None))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or("Unknown error".to_string());
            log::error!("Failed to fetch logs from API ({}): {}", status, error_text);
            return Err(self.server_error(None));
        }

        let logs_response: func::logs::Response = response
            .json()
            .await
            .wrap_err("Invalid response from server")
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        if logs_response.events.is_empty() {
            println!(
                "{}",
                console::style(format!(
                    "No logs found for this function in the last {}.",
                    self.command.period.clone().unwrap_or("1 hour".into())
                ))
                .yellow(),
            );

            return Ok(());
        }

        for event in logs_response.events {
            // Convert timestamp to readable format
            let datetime = match DateTime::<Utc>::from_timestamp_millis(event.timestamp) {
                Some(dt) => dt,
                None => {
                    log::warn!("Invalid timestamp: {}", event.timestamp);
                    continue;
                }
            };

            let formatted_time = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
            print!("{} {}", console::style(formatted_time).dim(), event.message);
        }

        Ok(())
    }
}
