use crate::client::Client;
use crate::crat::Crate;
use crate::error::Error;
use crate::function::Function;
use chrono::{DateTime, Utc};
use eyre::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LogEvent {
    timestamp: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct LogsResponse {
    events: Vec<LogEvent>,
}

/// Retrieves and displays logs for a specific function
pub async fn logs(function: &Function, crat: &Crate) -> Result<()> {
    let client = Client::new(false)?;

    println!(
        "{} {}\n",
        console::style("Fetching logs for").bold(),
        console::style(function.name()?).cyan()
    );

    let response = client
        .post("/function/logs")
        .json(&serde_json::json!({
            "crate_name": crat.name.clone(),
            "function_name": function.name()?
        }))
        .send()
        .await
        .wrap_err("Failed to send request to logs endpoint")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or("Unknown error".to_string());
        log::error!("Failed to fetch logs from API ({}): {}", status, error_text);
        return Err(Error::new("Failed to fetch logs", Some("Try again later.")).into());
    }

    let logs_response: LogsResponse = response.json().await.wrap_err(Error::new(
        "Invalid response from server",
        Some("Try again later."),
    ))?;

    if logs_response.events.is_empty() {
        println!(
            "{}",
            console::style("No logs found for this function in the last hour.").yellow(),
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
