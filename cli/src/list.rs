use crate::crat::Crate;
use kinetics_parser::{Parser, Role};
use serde_json::Value;
use std::collections::HashMap;
use syn::visit::Visit;
use tabled::settings::{peaker::Priority, style::Style, Settings, Width};
use tabled::{Table, Tabled};
use terminal_size::{terminal_size, Height as TerminalHeight, Width as TerminalWidth};
use walkdir::WalkDir;

#[derive(Tabled)]
struct EndpointRow {
    #[tabled(rename = "Function")]
    function: String,
    #[tabled(rename = "Environment")]
    environment: String,
    #[tabled(rename = "Url Path")]
    url_path: String,
}

#[derive(Tabled)]
struct CronRow {
    #[tabled(rename = "Function")]
    function: String,
    #[tabled(rename = "Environment")]
    environment: String,
    #[tabled(rename = "Schedule")]
    schedule: String,
}

#[derive(Tabled)]
struct WorkerRow {
    #[tabled(rename = "Function")]
    function: String,
    #[tabled(rename = "Environment")]
    environment: String,
    #[tabled(rename = "FIFO")]
    fifo: String,
    #[tabled(rename = "Concurrency")]
    concurrency: String,
    #[tabled(rename = "Queue Alias")]
    queue_alias: String,
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

pub async fn list(current_crate: &Crate) -> eyre::Result<()> {
    let mut parser = Parser::new();
    let domain = format!("https://{}.usekinetics.com", current_crate.name);

    for entry in WalkDir::new(&current_crate.path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
    {
        let content = std::fs::read_to_string(entry.path())?;
        let syntax = syn::parse_file(&content)?;

        parser.set_relative_path(entry.path().strip_prefix(&current_crate.path)?.to_str());
        parser.visit_file(&syntax);
    }

    let mut endpoint_rows = Vec::new();
    let mut cron_rows = Vec::new();
    let mut worker_rows = Vec::new();

    for parsed_function in parser.functions {
        let func_name = parsed_function.func_name(false);
        let func_path = parsed_function.relative_path.clone();

        match parsed_function.role {
            Role::Endpoint(params) => {
                endpoint_rows.push(EndpointRow {
                    function: format_function_and_path(&func_name, &func_path),
                    environment: format_environment(&format!("{:?}", params.environment)),
                    url_path: format!("{}{}", domain, params.url_path.unwrap_or("".to_string())),
                });
            }
            Role::Cron(params) => {
                cron_rows.push(CronRow {
                    function: format_function_and_path(&func_name, &func_path),
                    environment: format_environment(&format!("{:?}", params.environment)),
                    schedule: params.schedule.to_string(),
                });
            }
            Role::Worker(params) => {
                worker_rows.push(WorkerRow {
                    function: format_function_and_path(&func_name, &func_path),
                    environment: format_environment(&format!("{:?}", params.environment)),
                    fifo: format!("{:?}", params.fifo),
                    concurrency: format!("{:?}", params.concurrency),
                    queue_alias: format!("{:?}", params.queue_alias.unwrap_or("".to_string())),
                });
            }
        }
    }

    let (width, _) = get_terminal_size();

    let settings = Settings::default()
        .with(Width::wrap(width).priority(Priority::max(true)))
        .with(Width::increase(width));

    if !endpoint_rows.is_empty() {
        let mut table = Table::new(endpoint_rows);
        table.with(Style::modern()).with(settings.clone());
        println!("Endpoints:\n{}", table);
    }

    if !cron_rows.is_empty() {
        let mut table = Table::new(cron_rows);
        table.with(Style::modern()).with(settings.clone());
        println!("Crons:\n{}", table);
    }

    if !worker_rows.is_empty() {
        let mut table = Table::new(worker_rows);
        table.with(Style::modern()).with(settings.clone());
        println!("Workers:\n{}", table);
    }

    Ok(())
}
