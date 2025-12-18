use crate::error::Error;
use crate::function::Function;
use crate::project::Project;
use crate::{api::func, client::Client};
use color_eyre::owo_colors::OwoColorize as _;
use eyre::{Context, Result};
use kinetics_parser::Parser;

/// Retrieves and displays run statistics for a specific function
pub async fn stats(function_name: &str, project: &Project, period: u32) -> Result<()> {
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
        console::style("Fetching stats").bold().green(),
        console::style("for").dim(),
        console::style(&function.name).bold()
    );

    let response = client
        .post("/function/stats")
        .json(&func::stats::Request {
            project_name: project.name.to_owned(),
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
