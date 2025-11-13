use crate::client::Client;
use crate::crat::Crate;
use crate::function::Function;
use crate::project::Project;
use color_eyre::owo_colors::OwoColorize;
use kinetics_parser::{ParsedFunction, Parser, Role};
use serde_json::Value;
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

fn verbose(
    endpoint_rows: &[EndpointRow],
    cron_rows: &[CronRow],
    worker_rows: &[WorkerRow],
    width: usize,
) {
    // Verbose output with tables
    let settings = Settings::default()
        .with(Width::wrap(width).priority(Priority::max(true)))
        .with(Width::increase(width));

    if !endpoint_rows.is_empty() {
        let mut table = Table::new(endpoint_rows.to_vec());
        table.with(Style::modern()).with(settings.clone());
        println!("Endpoints\n{}", table);
    }

    if !cron_rows.is_empty() {
        let mut table = Table::new(cron_rows.to_vec());
        table.with(Style::modern()).with(settings.clone());
        println!("Crons:\n{}", table);
    }

    if !worker_rows.is_empty() {
        let mut table = Table::new(worker_rows.to_vec());
        table.with(Style::modern()).with(settings);
        println!("Workers:\n{}", table);
    }
}

/// Display the function with its main properties
pub fn display_simple(function: &ParsedFunction, project_base_url: &str) -> eyre::Result<()> {
    println!(
        "{} {} {}",
        function.func_name(false)?.bold(),
        "from".dimmed(),
        function.relative_path.dimmed(),
    );

    match function.role.clone() {
        Role::Endpoint(params) => {
            if let Some(url_path) = params.url_path {
                println!("{}", format!("{}{}", project_base_url, url_path).cyan());
            }
        }

        Role::Cron(params) => println!("{} {}", "Scheduled".dimmed(), params.schedule.cyan()),
        Role::Worker(_) => {}
    }

    println!();
    Ok(())
}

fn simple(functions: &[ParsedFunction], project_base_url: &str) -> eyre::Result<()> {
    let crons: Vec<&ParsedFunction> = functions
        .iter()
        .filter(|f| matches!(f.role, Role::Cron(_)))
        .collect();

    let endpoints: Vec<&ParsedFunction> = functions
        .iter()
        .filter(|f| matches!(f.role, Role::Endpoint(_)))
        .collect();

    let workers: Vec<&ParsedFunction> = functions
        .iter()
        .filter(|f| matches!(f.role, Role::Worker(_)))
        .collect();

    if !endpoints.is_empty() {
        println!("\n{}\n", "Endpoints".bold().green());

        endpoints
            .iter()
            .try_for_each(|f| display_simple(f, project_base_url))?;
    }

    if !workers.is_empty() {
        println!("{}\n", "Workers".bold().green());

        workers
            .iter()
            .try_for_each(|f| display_simple(f, project_base_url))?;
    }

    if !crons.is_empty() {
        println!("{}\n", "Crons".bold().green());

        crons
            .iter()
            .try_for_each(|f| display_simple(f, project_base_url))?;
    }

    Ok(())
}

/// Prints out the list of all functions
///
/// With some extra information
pub async fn list(current_crate: &Crate, is_verbose: bool) -> eyre::Result<()> {
    let functions = Parser::new(Some(&current_crate.path))?.functions;
    let project_base_url = Project::fetch_one(&current_crate.project.name).await?.url;

    if !is_verbose {
        return simple(&functions, &project_base_url);
    }

    let mut endpoint_rows = Vec::new();
    let mut cron_rows = Vec::new();
    let mut worker_rows = Vec::new();
    let client = Client::new(false).await?;

    if functions.is_empty() {
        println!("{}", console::style("No functions found").yellow());
        return Ok(());
    }

    for parsed_function in functions {
        let function = Function::new(current_crate, &parsed_function)?;

        let last_modified = function
            .status(&client)
            .await?
            .unwrap_or_else(|| "NA".into());

        let func_path = parsed_function.relative_path;

        match parsed_function.role {
            Role::Endpoint(params) => {
                endpoint_rows.push(EndpointRow {
                    function: format_function_and_path(&function.name, &func_path),
                    environment: format_environment(&format!("{:?}", params.environment)),
                    url_path: format!(
                        "{}{}",
                        project_base_url,
                        params.url_path.unwrap_or("".to_string())
                    ),
                    last_modified,
                });
            }
            Role::Cron(params) => {
                cron_rows.push(CronRow {
                    function: format_function_and_path(&function.name, &func_path),
                    environment: format_environment(&format!("{:?}", params.environment)),
                    schedule: params.schedule.to_string(),
                    last_modified,
                });
            }
            Role::Worker(params) => {
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

    verbose(&endpoint_rows, &cron_rows, &worker_rows, width);

    Ok(())
}
