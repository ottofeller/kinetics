use crate::client::Client;
use crate::error::Error;
use crate::function::Function;
use crate::project::Project;
use eyre::{Context, Result};
use http::StatusCode;
use kinetics_parser::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct StartRequest {
    pub project_name: String,
    pub function_name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StartResponse {
    /// Datetime when throttling was applied
    throttled_at: String,

    /// The reason for throttling,
    /// e.g. user request or account limit.
    reason: String,
}

/// Removes throttling from a function.
/// The function starts receiving requests.
pub async fn start(function_name: &str, project: &Project) -> Result<()> {
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
        console::style("Calling start").bold().green(),
        console::style("on").dim(),
        console::style(&function.name).bold()
    );

    let response = client
        .post("/function/start")
        .json(&StartRequest {
            project_name: project.name.clone(),
            function_name: function.name,
        })
        .send()
        .await
        .wrap_err("Failed to send request to start endpoint")?;

    match response.status() {
        status if status.is_success() => Ok(()),
        StatusCode::NOT_MODIFIED => {
            println!(
                "{}",
                console::style(format!("Nothing changed. Function is not throttled.")).yellow(),
            );

            Ok(())
        }
        StatusCode::FORBIDDEN => {
            let start_response: StartResponse = response.json().await.wrap_err(Error::new(
                "Invalid response from server",
                Some("Try again later."),
            ))?;

            println!(
                "{}",
                console::style(format!(
                    "Function is stopped by platform. {}",
                    start_response.reason
                ))
                .yellow(),
            );

            Ok(())
        }
        err_status => {
            let error_text = response.text().await.unwrap_or("Unknown error".to_string());
            log::error!(
                "Failed to call start from API ({}): {}",
                err_status,
                error_text
            );
            Err(Error::new("Failed to call start", Some("Try again later.")).into())
        }
    }
}
