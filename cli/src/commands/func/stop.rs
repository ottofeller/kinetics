use crate::client::Client;
use crate::error::Error;
use crate::function::Function;
use crate::project::Project;
use eyre::{Context, Result};
use http::StatusCode;
use kinetics_parser::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct StopRequest {
    pub project_name: String,
    pub function_name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StopResponse {
    /// Datetime when throttling was applied
    pub throttled_at: String,

    /// The reason for throttling,
    /// e.g. user request or account limit.
    pub reason: String,
}

/// Applies throttling to a function.
///
/// The function stop receiving requests.
/// The endpoint starts responding "Service Unavailable".
pub async fn stop(function_name: &str, project: &Project) -> Result<()> {
    // Get all function names without any additional manupulations.
    let all_functions = Parser::new(Some(&project.path))?
        .functions
        .into_iter()
        .map(|f| Function::new(project, &f))
        .collect::<eyre::Result<Vec<Function>>>()?;
    let function = Function::find_by_name(&all_functions, function_name)?;

    let client = Client::new(false).await?;

    println!(
        "\n{} {} {}...\n",
        console::style("Calling stop").bold().green(),
        console::style("on").dim(),
        console::style(&function.name).bold()
    );

    let response = client
        .post("/function/stop")
        .json(&StopRequest {
            project_name: project.name.clone(),
            function_name: function.name,
        })
        .send()
        .await
        .inspect_err(|err| log::error!("{err:?}"))
        .wrap_err("Failed to send request to stop endpoint")?;

    match response.status() {
        status if status.is_success() => Ok(()),
        StatusCode::NOT_MODIFIED => {
            let stop_response: StopResponse = response
                .json()
                .await
                .inspect_err(|err| log::error!("{err:?}"))
                .wrap_err(Error::new(
                    "Invalid response from server",
                    Some("Try again later."),
                ))?;

            println!(
                "{}",
                console::style(format!(
                    "Nothing changed. Function already throttled at {} - {}.",
                    stop_response.throttled_at, stop_response.reason
                ))
                .yellow(),
            );
            Ok(())
        }
        err_status => {
            let error_text = response.text().await.unwrap_or("Unknown error".to_string());
            log::error!("Failed to call stop from API ({err_status}): {error_text}");
            Err(Error::new("Failed to call stop", Some("Try again later.")).into())
        }
    }
}
