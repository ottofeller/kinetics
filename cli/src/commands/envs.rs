use super::build::prepare_functions;
use crate::client::Client;
use crate::config::build_config;
use crate::project::Project;
use crossterm::style::Stylize;
use eyre::{eyre, WrapErr};
use kinetics_parser::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Request body for /envs/list
#[derive(Serialize)]
struct EnvsListRequest {
    project_name: String,
}

/// Response from /envs/list
#[derive(Deserialize)]
struct EnvsListResponse {
    functions: HashMap<String, HashMap<String, String>>,
}

/// Lists all environment variables for all functions in the current crate
pub async fn list(project: &Project, is_remote: bool) -> eyre::Result<()> {
    let parsed_functions = Parser::new(Some(&project.path))?.functions;
    println!();

    let envs = if is_remote {
        println!("{}\n", "Fetching deployed env vars".bold().green());
        remote(project).await?
    } else {
        println!("{}\n", "Showing local env vars".bold().green());
        local(project).await?
    };

    if envs.is_empty() {
        println!("{}", "No envs found".yellow());
        return Ok(());
    }

    for (function_name, env_vars) in envs {
        if env_vars.is_empty() {
            continue;
        }

        let path = parsed_functions
            .iter()
            .find(|f| {
                f.func_name(false)
                    .inspect_err(|e| log::error!("Failed to get function name: {e:?}"))
                    .unwrap()
                    == function_name
            })
            .ok_or(eyre!("Parsed artifact has no function name"))
            .inspect_err(|e| log::error!("{e:?}"))?
            .relative_path
            .to_owned();

        println!(
            "{} {}",
            function_name.bold(),
            format!("from {}", path).dim()
        );

        for (key, value) in env_vars {
            println!("{} {}", key.dim(), value.black());
        }

        println!();
    }

    Ok(())
}

/// Gets environment variables from the backend
async fn remote(project: &Project) -> eyre::Result<HashMap<String, HashMap<String, String>>> {
    let response = Client::new(false)
        .await?
        .post("/envs/list")
        .json(&EnvsListRequest {
            project_name: project.name.to_owned(),
        })
        .send()
        .await
        .wrap_err("Failed to send request to /envs/list endpoint")?;

    if !response.status().is_success() {
        log::error!(
            "Failed to fetch envs from API ({}): {}",
            response.status(),
            response.text().await.unwrap_or("Unknown error".to_string())
        );

        return Err(eyre::eyre!("Failed to fetch envs from backend"));
    }

    let response: EnvsListResponse = response
        .json()
        .await
        .wrap_err("Failed to parse response from the backend as JSON")?;

    Ok(response.functions)
}

/// Gets environment variables from local configuration
async fn local(project: &Project) -> eyre::Result<HashMap<String, HashMap<String, String>>> {
    let functions = prepare_functions(PathBuf::from(build_config()?.kinetics_path), project, &[])?;
    let mut result = HashMap::new();

    for function in functions {
        let function_name = function.name.clone();

        // Get environment variables for this function
        let env_vars = function.environment();

        if !env_vars.is_empty() {
            result.insert(function_name, env_vars.clone());
        }
    }

    Ok(result)
}
