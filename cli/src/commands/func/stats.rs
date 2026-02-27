use crate::api::func;
use crate::error::Error;
use crate::function::Function;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use color_eyre::owo_colors::OwoColorize as _;
use eyre::Context;
use kinetics_parser::Parser;

#[derive(clap::Args, Clone)]
pub(crate) struct StatsCommand {
    /// Function name to get statistics for.
    /// Run `kinetics list` to get a complete list of function names in a project.
    #[arg()]
    name: String,

    /// Period to get statistics for.
    ///
    /// The period object (e.g. `1day 3hours`) is a concatenation of time spans.
    /// Where each time span is an integer number and a suffix representing time units.
    ///
    /// Maximum available period is 7 days.
    /// Defaults to 1day.
    ///
    #[arg(short, long)]
    period: Option<String>,
}

impl Runnable for StatsCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        StatsRunner {
            command: self.clone(),
        }
    }
}

struct StatsRunner {
    command: StatsCommand,
}

impl Runner for StatsRunner {
    /// Retrieves and displays run statistics for a specific function
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        // Get all function names without any additional manipulations.
        let all_functions = Parser::new(Some(&project.path))?
            .functions
            .into_iter()
            .map(|f| Function::new(&project, &f))
            .collect::<eyre::Result<Vec<Function>>>()?;
        let function = Function::find_by_name(&all_functions, &self.command.name)?;

        let client = self.api_client().await?;

        println!(
            "\n{} {} {}...\n",
            console::style("Fetching stats").bold().green(),
            console::style("for").dim(),
            console::style(&function.name).bold()
        );

        let response = client
            .post("/function/stats")
            .json(&func::stats::Request {
                project_name: project.name.to_owned(),
                function_name: function.name,
                period: self.command.period.to_owned(),
            })
            .send()
            .await
            .wrap_err("Failed to send request to stat endpoint")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or("Unknown error".to_string());
            log::error!(
                "Failed to fetch statistics from API ({}): {}",
                status,
                error_text
            );
            return Err(Error::new("Failed to fetch statistics", Some("Try again later.")).into());
        }

        let logs_response: func::stats::Response = response.json().await.wrap_err(Error::new(
            "Invalid response from server",
            Some("Try again later."),
        ))?;

        println!("{}", "Runs:".bold());
        println!("  Total: {}", logs_response.runs.total);
        println!("  Success: {}", logs_response.runs.success);
        println!("  Error: {}", logs_response.runs.error);
        Ok(())
    }
}
