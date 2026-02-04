use crate::api::func;
use crate::api::client::Client;
use crate::error::Error;
use crate::function::Function;
use crate::project::Project;
use eyre::{Context, Result};
use http::StatusCode;
use kinetics_parser::Parser;
use std::*;

/// Adds/removes throttling from a function.
///
/// The actual operation is provided as the third argument.
/// - For start operation the function starts receiving requests.
/// - For stop operation the function stop receiving requests
///   and the endpoint starts responding "Service Unavailable".
pub async fn toggle(function_name: &str, project: &Project, op: func::toggle::Op) -> Result<()> {
    // Get all function names without any additional manupulations.
    let all_functions = Parser::new(Some(&project.path))?
        .functions
        .into_iter()
        .map(|f| Function::new(project, &f))
        .collect::<eyre::Result<Vec<Function>>>()?;
    let function = Function::find_by_name(&all_functions, function_name)?;

    let client = Client::new(false).await?;

    println!(
        "\n{} {}...\n",
        console::style(format!("{op}")).bold().green(),
        console::style(&function.name).bold()
    );

    let response = client
        .post("/function/toggle")
        .json(&func::toggle::Request {
            project_name: project.name.clone(),
            function_name: function.name,
            operation: op.clone(),
        })
        .send()
        .await
        .wrap_err(Error::new(
            &format!("Failed to send {op:?} request"),
            Some("Try again later."),
        ))?;

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
                        func::toggle::Op::Start => "not",
                        func::toggle::Op::Stop => "already",
                    }
                ))
                .yellow(),
            );

            Ok(())
        }
        StatusCode::FORBIDDEN => {
            let func::toggle::Response { reason, .. } = response.json().await.wrap_err(
                Error::new("Invalid response from server", Some("Try again later.")),
            )?;

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
