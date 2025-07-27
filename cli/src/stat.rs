use crate::crat::Crate;
use crate::error::Error;
use crate::function::Function;
use crate::{client::Client, function::project_functions};
use color_eyre::owo_colors::OwoColorize as _;
use eyre::{Context, Result};
use serde::{Deserialize, Serialize};

/// Request
#[derive(Serialize)]
struct RequestBody {
    crate_name: String,
    function_name: String,
    /// The period (measured in days) to get statistics for
    period: u32,
}

/// Response
#[derive(Deserialize)]
struct JsonResponse {
    runs: Runs,
    last_call: Option<LastCall>,
}

#[derive(Deserialize)]
struct Runs {
    success: u64,
    error: u64,
    total: u64,
}

#[derive(Deserialize)]
struct LastCall {
    timestamp: String,
    status: String,
}

/// Retrieves and displays run statistics for a specific function
pub async fn stat(function_name: &str, crat: &Crate, period: u32) -> Result<()> {
    // Get all function names without any additional manupulations.
    let all_functions = project_functions(crat)?
        .into_iter()
        .map(|f| Function::new(&crat.path, &f.func_name(false)))
        .collect::<eyre::Result<Vec<Function>>>()?;
    let function = Function::find_by_name(&all_functions, function_name)?;

    let client = Client::new(false)?;

    println!(
        "\n{} {} {}...\n",
        console::style("Fetching statistics").bold().green(),
        console::style("for").dim(),
        console::style(&function.name).bold()
    );

    let response = client
        .post("/function/stat")
        .json(&RequestBody {
            crate_name: crat.name.to_owned(),
            function_name: function.name,
            period,
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

    let logs_response: JsonResponse = response.json().await.wrap_err(Error::new(
        "Invalid response from server",
        Some("Try again later."),
    ))?;

    println!("{}", "Runs:".bold().green());
    println!("  total: {}", logs_response.runs.total);
    println!("  success: {}", logs_response.runs.success);
    println!("  error: {}", logs_response.runs.error);

    print!("{}", "Last called:".bold().green());
    let Some(last_call) = logs_response.last_call else {
        print!(" NA");
        println!();
        return Ok(());
    };

    println!();
    println!("  status: {}", last_call.status);
    println!("  timestamp: {}", last_call.timestamp);
    Ok(())
}
