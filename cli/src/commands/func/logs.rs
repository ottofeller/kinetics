use crate::error::Error;
use crate::function::Function;
use crate::project::Project;
use crate::api::{func, client::Client};
use chrono::{DateTime, Utc};
use eyre::{Context, Result};
use kinetics_parser::Parser;

/// Retrieves and displays logs for a specific function
pub async fn logs(function_name: &str, project: &Project, period: &Option<String>) -> Result<()> {
    // Get all function names without any additional manipulations.
    let all_functions = Parser::new(Some(&project.path))?
        .functions
        .into_iter()
        .map(|f| Function::new(project, &f))
        .collect::<eyre::Result<Vec<Function>>>()?;
    let function = Function::find_by_name(&all_functions, function_name)?;

    let client = Client::new(false).await?;

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
            period: period.to_owned(),
        })
        .send()
        .await
        .wrap_err("Failed to send request to logs endpoint")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or("Unknown error".to_string());
        log::error!("Failed to fetch logs from API ({}): {}", status, error_text);
        return Err(Error::new("Failed to fetch logs", Some("Try again later.")).into());
    }

    let logs_response: func::logs::Response = response.json().await.wrap_err(Error::new(
        "Invalid response from server",
        Some("Try again later."),
    ))?;

    if logs_response.events.is_empty() {
        println!(
            "{}",
            console::style(format!(
                "No logs found for this function in the last {}.",
                period.clone().unwrap_or("1 hour".into())
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
