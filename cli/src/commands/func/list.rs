use crate::api::client::Client;
use crate::error::Error;
use crate::function::Function;
use crate::project::Project;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use color_eyre::owo_colors::OwoColorize;
use eyre::Context;
use kinetics_parser::{Params, ParsedFunction, Role};
use serde_json::{json, Value};
use std::collections::HashMap;
use tabled::settings::{peaker::Priority, style::Style, Settings, Width};
use tabled::{Table, Tabled};
use terminal_size::{terminal_size, Height as TerminalHeight, Width as TerminalWidth};

#[derive(Tabled, Clone)]
struct EndpointRow {
    #[tabled(rename = "Function")]
    function: String,
    #[tabled(rename = "Environment")]
    environment: String,
    #[tabled(rename = "Url Path")]
    url_path: String,
    #[tabled(rename = "Updated")]
    last_modified: String,
}

#[derive(Tabled, Clone)]
struct CronRow {
    #[tabled(rename = "Function")]
    function: String,
    #[tabled(rename = "Environment")]
    environment: String,
    #[tabled(rename = "Schedule")]
    schedule: String,
    #[tabled(rename = "Updated")]
    last_modified: String,
}

#[derive(Tabled, Clone)]
struct WorkerRow {
    #[tabled(rename = "Function")]
    function: String,
    #[tabled(rename = "Environment")]
    environment: String,
    #[tabled(rename = "FIFO")]
    fifo: String,
    #[tabled(rename = "Concurrency")]
    concurrency: String,
    #[tabled(rename = "Updated")]
    last_modified: String,
}

#[derive(clap::Args, Clone)]
pub(crate) struct ListCommand {
    /// Show detailed information for each function
    #[arg(short, long)]
    verbose: bool,
}

impl Runnable for ListCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        ListRunner {
            command: self.clone(),
            functions: vec![],
            writer,
        }
    }
}

struct ListRunner<'a> {
    functions: Vec<ParsedFunction>,
    command: ListCommand,
    writer: &'a Writer,
}

impl Runner for ListRunner<'_> {
    /// Prints out the list of all functions with some extra information
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        // Initialize client early and fail with clear error if user's logged out
        // If the method is called within other method, then the auth error won't be propogated
        let client = self.api_client().await?;

        self.functions = project
            .parsed_functions()
            .wrap_err("Failed to parse the project")
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        if !self.command.verbose {
            return self
                .simple()
                .wrap_err("Failed to output the simple list")
                .map_err(|e| self.error(None, None, Some(e.into())));
        }

        self.verbose(&client)
            .await
            .wrap_err("Failed to output the verbose list")
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        Ok(())
    }
}

impl ListRunner<'_> {
    fn simple(&self) -> eyre::Result<()> {
        let crons: Vec<&ParsedFunction> = self
            .functions
            .iter()
            .filter(|f| matches!(f.role, Role::Cron))
            .collect();

        let endpoints: Vec<&ParsedFunction> = self
            .functions
            .iter()
            .filter(|f| matches!(f.role, Role::Endpoint))
            .collect();

        let workers: Vec<&ParsedFunction> = self
            .functions
            .iter()
            .filter(|f| matches!(f.role, Role::Worker))
            .collect();

        if !endpoints.is_empty() {
            self.writer
                .text(&format!("\n{}\n\n", "Endpoints".bold().green()))
                .map_err(|e| eyre::eyre!(e))?;

            endpoints.iter().try_for_each(|f| self.display_simple(f))?;
        }

        if !workers.is_empty() {
            self.writer
                .text(&format!("\n{}\n\n", "Workers".bold().green()))
                .map_err(|e| eyre::eyre!(e))?;

            workers.iter().try_for_each(|f| self.display_simple(f))?;
        }

        if !crons.is_empty() {
            self.writer
                .text(&format!("\n{}\n\n", "Crons".bold().green()))
                .map_err(|e| eyre::eyre!(e))?;

            crons.iter().try_for_each(|f| self.display_simple(f))?;
        }

        let mut functions_json: Vec<Value> = vec![];

        for f in &self.functions {
            let mut entry = json!({
                "name": f.func_name(false)?,
                "role": format!("{:?}", f.role).to_lowercase(),
                "path": f.to_string(),
            });

            if let Params::Cron(ref params) = f.params {
                entry["schedule"] = json!(params.schedule.to_string());
            }

            functions_json.push(entry);
        }

        self.writer
            .json(json!({"success": true, "functions": functions_json}))
            .map_err(|e| eyre::eyre!(e))?;

        Ok(())
    }

    async fn verbose(&mut self, client: &Client) -> eyre::Result<()> {
        let project = self.project().await?;
        let project_base_url = Project::fetch_one(&project.name).await?.url;
        let mut endpoint_rows = Vec::new();
        let mut cron_rows = Vec::new();
        let mut worker_rows = Vec::new();

        if self.functions.is_empty() {
            self.writer
                .text(&format!(
                    "{}\n",
                    console::style("No functions found").yellow()
                ))
                .map_err(|e| eyre::eyre!(e))?;

            self.writer
                .json(json!({"success": true, "functions": []}))
                .map_err(|e| eyre::eyre!(e))?;

            return Ok(());
        }

        for parsed_function in self.functions.clone() {
            let function = Function::new(&project, &parsed_function)?;

            let last_modified = function
                .status(client)
                .await?
                .unwrap_or_else(|| "NA".into());

            let func_path = parsed_function.to_string();

            match parsed_function.params {
                Params::Endpoint(params) => {
                    endpoint_rows.push(EndpointRow {
                        function: format_function_and_path(&function.name, &func_path),
                        environment: format_environment(&format!("{:?}", params.environment)),
                        url_path: format!("{}{}", project_base_url, params.url_path),
                        last_modified,
                    });
                }
                Params::Cron(params) => {
                    cron_rows.push(CronRow {
                        function: format_function_and_path(&function.name, &func_path),
                        environment: format_environment(&format!("{:?}", params.environment)),
                        schedule: params.schedule.to_string(),
                        last_modified,
                    });
                }
                Params::Worker(params) => {
                    worker_rows.push(WorkerRow {
                        function: format_function_and_path(&function.name, &func_path),
                        environment: format_environment(&format!("{:?}", params.environment)),
                        fifo: format!("{:?}", params.fifo),
                        concurrency: format!("{:?}", params.concurrency),
                        last_modified,
                    });
                }
            }
        }

        let (width, _) = get_terminal_size();

        // Verbose output with tables
        let settings = Settings::default()
            .with(Width::wrap(width).priority(Priority::max(true)))
            .with(Width::increase(width));

        if !endpoint_rows.is_empty() {
            let mut table = Table::new(endpoint_rows.to_vec());
            table.with(Style::modern()).with(settings.clone());

            self.writer
                .text(&format!("Endpoints\n{}\n", table))
                .map_err(|e| eyre::eyre!(e))?;
        }

        if !cron_rows.is_empty() {
            let mut table = Table::new(cron_rows.to_vec());
            table.with(Style::modern()).with(settings.clone());

            self.writer
                .text(&format!("Crons:\n{}\n", table))
                .map_err(|e| eyre::eyre!(e))?;
        }

        if !worker_rows.is_empty() {
            let mut table = Table::new(worker_rows.to_vec());
            table.with(Style::modern()).with(settings);
            self.writer
                .text(&format!("Workers:\n{}\n", table))
                .map_err(|e| eyre::eyre!(e))?;
        }

        let mut functions_json: Vec<Value> = vec![];

        for row in &endpoint_rows {
            functions_json.push(json!({
                "role": "endpoint",
                "function": &row.function,
                "environment": &row.environment,
                "url_path": &row.url_path,
                "last_modified": &row.last_modified,
            }));
        }

        for row in &cron_rows {
            functions_json.push(json!({
                "role": "cron",
                "function": &row.function,
                "environment": &row.environment,
                "schedule": &row.schedule,
                "last_modified": &row.last_modified,
            }));
        }

        for row in &worker_rows {
            functions_json.push(json!({
                "role": "worker",
                "function": &row.function,
                "environment": &row.environment,
                "fifo": &row.fifo,
                "concurrency": &row.concurrency,
                "last_modified": &row.last_modified,
            }));
        }

        self.writer
            .json(json!({"success": true, "functions": functions_json}))
            .map_err(|e| eyre::eyre!(e))?;

        Ok(())
    }

    /// Display the function with its main properties
    fn display_simple(&self, function: &ParsedFunction) -> eyre::Result<()> {
        self.writer
            .text(&format!(
                "{} {}\n",
                function.func_name(false)?.bold(),
                function.to_string().dimmed(),
            ))
            .map_err(|e| eyre::eyre!(e))?;

        match function.params.clone() {
            Params::Endpoint(_) => {}
            Params::Cron(params) => {
                self.writer
                    .text(&format!("{}\n", params.schedule.cyan()))
                    .map_err(|e| eyre::eyre!(e))?;
            }
            Params::Worker(_) => {}
        }

        Ok(())
    }
}

fn format_environment(json_str: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<HashMap<String, Value>>(json_str) {
        parsed
            .into_iter()
            .map(|(key, value)| format!("{}: {}", key, value))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        json_str.to_string() // Fallback to the original string if parsing fails
    }
}

fn format_function_and_path(function: &str, path: &str) -> String {
    format!("{}\n({})", function, path)
}

fn get_terminal_size() -> (usize, usize) {
    let (TerminalWidth(width), TerminalHeight(height)) =
        terminal_size().expect("failed to obtain a terminal size");

    (width as usize, height as usize)
}
