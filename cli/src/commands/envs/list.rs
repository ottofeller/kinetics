use crate::api::client::Client;
use crate::api::envs;
use crate::config::build_config;
use crate::error::Error;
use crate::project::Project;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use crossterm::style::Stylize;
use eyre::{eyre, WrapErr};
use kinetics_parser::{ParsedFunction, Parser};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(clap::Args, Clone)]
pub(crate) struct ListCommand {
    /// When passed shows env vars used by deployed functions
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    remote: bool,
}

impl Runnable for ListCommand {
    fn runner(&self, _writer: &Writer) -> impl Runner {
        ListRunner {
            command: self.clone(),
        }
    }
}

struct ListRunner {
    command: ListCommand,
}

impl Runner for ListRunner {
    /// Lists all environment variables for all functions in the current crate
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;
        let parsed_functions = Parser::new(Some(&project.path))
            .map_err(|e| self.error(None, None, Some(e.into())))?
            .functions;

        println!();

        let envs = if self.command.remote {
            println!(
                "{}...\n",
                console::style("Fetching env vars").green().bold()
            );

            remote(&project, &parsed_functions)
                .await
                .map_err(|e| self.server_error(Some(e.into())))?
        } else {
            println!("{}\n", "Showing local env vars".bold().green());

            local(&project)
                .await
                .map_err(|e| self.error(None, None, Some(e.into())))?
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
                        .inspect_err(|e| log::error!("Error: {e:?}"))
                        .wrap_err("Failed to process functions")
                        .unwrap()
                        == function_name
                })
                .ok_or(eyre!("Parsed artifact has no function name"))
                .map_err(|e| self.error(None, None, Some(e.into())))?
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
}

/// Gets environment variables from the backend
async fn remote(
    project: &Project,
    functions: &[ParsedFunction],
) -> eyre::Result<HashMap<String, HashMap<String, String>>> {
    let response = Client::new(false)
        .await?
        .post("/envs/list")
        .json(&envs::list::Request {
            project_name: project.name.to_owned(),
            functions_names: functions
                .iter()
                .map(|f| f.func_name(false))
                .collect::<eyre::Result<Vec<String>>>()?,
        })
        .send()
        .await
        .wrap_err("Failed to send request to /envs/list endpoint")?;

    if !response.status().is_success() {
        log::error!(
            "Error for status {}: {}",
            response.status(),
            response.text().await.unwrap_or("Unknown error".to_string())
        );

        return Err(eyre::eyre!("Failed to fetch envs from backend"));
    }

    let response: envs::list::Response = response
        .json()
        .await
        .inspect_err(|e| log::error!("JSON parse error: {e:?}"))
        .wrap_err("Request failed")?;

    Ok(response)
}

/// Gets environment variables from local configuration
async fn local(project: &Project) -> eyre::Result<HashMap<String, HashMap<String, String>>> {
    let functions = project.parse(PathBuf::from(build_config()?.kinetics_path), &[])?;
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
