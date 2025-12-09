use crate::client::Client;
use crate::error::Error;
use crate::function::Function;
use crate::project::Project;
use eyre::{Context, Result};
use http::StatusCode;
use kinetics_parser::Parser;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum ToggleOp {
    Start,
    Stop,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ToggleRequest {
    pub project_name: String,
    pub function_name: String,
    pub operation: ToggleOp,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ToggleResponse {
    /// Datetime when throttling was applied
    pub throttled_at: String,

    /// The reason for throttling,
    /// e.g. user request or account limit.
    pub reason: String,
}

/// Adds/removes throttling from a function.
///
/// The actual operation is provided as the third argument.
/// - For start operation the function starts receiving requests.
/// - For stop operation the function stop receiving requests
/// and the endpoint starts responding "Service Unavailable".
pub async fn toggle(function_name: &str, project: &Project, op: ToggleOp) -> Result<()> {
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
        console::style(format!("Calling {op:?}")).bold().green(),
        console::style("on").dim(),
        console::style(&function.name).bold()
    );

    let response = client
        .post("/function/toggle")
        .json(&ToggleRequest {
            project_name: project.name.clone(),
            function_name: function.name,
            operation: op.clone(),
        })
        .send()
        .await
        .inspect_err(|err| log::error!("{err:?}"))
        .wrap_err("Failed to send request to start endpoint")?;

    match response.status() {
        status if status.is_success() => {
            println!("{}", console::style("Done").bold().green());
            Ok(())
        }
        StatusCode::NOT_MODIFIED => {
            println!(
                "{}",
                console::style(format!(
                    "Nothing changed. Function is {} throttled.",
                    match op {
                        ToggleOp::Start => "not",
                        ToggleOp::Stop => "already",
                    }
                ))
                .yellow(),
            );

            Ok(())
        }
        StatusCode::FORBIDDEN => {
            let ToggleResponse { reason, .. } = response
                .json()
                .await
                .inspect_err(|err| log::error!("{err:?}"))
                .wrap_err(Error::new(
                    "Invalid response from server",
                    Some("Try again later."),
                ))?;

            println!(
                "{}",
                console::style(format!("Function is stopped by platform. {reason}")).yellow(),
            );

            Ok(())
        }
        err_status => {
            let error_text = response.text().await.unwrap_or("Unknown error".to_string());
            log::error!("Failed to call {op:?} from API ({err_status}): {error_text}");
            Err(Error::new(&format!("Failed to call {op:?}"), Some("Try again later.")).into())
        }
    }
}
