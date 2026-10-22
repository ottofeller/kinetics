use super::check_results;
use crate::api::client::Client;
use crate::api::error::ApiError;
use crate::error::Error;
use crate::project::Project;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use crossterm::style::Stylize;
use eyre::{eyre, WrapErr};
use kinetics_api::envs;
use kinetics_parser::ParsedFunction;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(clap::Args, Clone)]
pub(crate) struct ListCommand {
    /// When passed shows env vars used by deployed functions
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    remote: bool,

    /// Relative path to the project directory
    #[arg(long)]
    project: Option<PathBuf>,
}

impl Runnable for ListCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        ListRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct ListRunner<'a> {
    command: ListCommand,
    writer: &'a Writer,
}

impl Runner for ListRunner<'_> {
    /// Lists all environment variables for all functions in the current crate
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project(&self.command.project).await?;
        let parsed_functions = project
            .parsed_functions()
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        self.writer.text("\n")?;

        let response = if self.command.remote {
            self.writer.text(&format!(
                "{} env vars...\n\n",
                console::style("Fetching").bold()
            ))?;

            remote(&project, &parsed_functions)
                .await
                .map_err(|e| self.server_error(Some(e.into())))?
        } else {
            self.writer.text(&format!(
                "{} local env vars\n\n",
                console::style("Showing").bold()
            ))?;

            let envs = local(&project)
                .await
                .map_err(|e| self.error(None, None, Some(e.into())))?;
            envs::list::Response {
                results: envs
                    .into_iter()
                    .map(|(name, envs)| (name, envs::FunctionResult::Success { envs: Some(envs) }))
                    .collect(),
            }
        };

        let mut envs = HashMap::new();
        for (name, result) in &response.results {
            match result {
                envs::FunctionResult::Success {
                    envs: Some(variables),
                } => {
                    envs.insert(name.clone(), variables.clone());
                }
                envs::FunctionResult::Success { envs: None } => {
                    return Err(Error::new(
                        "Invalid response from server",
                        Some("Environment variables are missing from a successful result."),
                    ));
                }
                envs::FunctionResult::Error { .. } => {}
            }
        }

        let mut envs_json: HashMap<String, Value> = HashMap::new();

        for (function_name, env_vars) in &envs {
            envs_json.insert(function_name.clone(), json!(env_vars));
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
                        == *function_name
                })
                .ok_or(eyre!("Parsed artifact has no function name"))
                .map_err(|e| self.error(None, None, Some(e.into())))?
                .to_string();

            self.writer.text(&format!(
                "{function_name} {} {}\n",
                "from".dim(),
                path.dim().underlined()
            ))?;

            for (key, value) in env_vars {
                self.writer.text(&format!(
                    "{} {}\n",
                    key.as_str().dim(),
                    value.as_str().black()
                ))?;
            }

            self.writer.text("\n")?;
        }

        if (response.results.is_empty() || !envs.is_empty()) && envs.values().all(HashMap::is_empty)
        {
            self.writer
                .text(&format!("{}\n", "No envs found".yellow()))?;
        }

        check_results(&response, "Failed to read environment variables")?;
        self.writer.json(json!({
            "success": true,
            "envs": envs_json,
            "results": response.results,
        }))?;

        Ok(())
    }
}

/// Gets environment variables from the backend
async fn remote(
    project: &Project,
    functions: &[ParsedFunction],
) -> eyre::Result<envs::list::Response> {
    let response = Client::new(false)
        .await?
        .post("/envs/list")
        .json(&envs::list::Request {
            project: project.into(),
            detailed_results: true,
            functions_names: functions
                .iter()
                .map(|f| f.func_name(false))
                .collect::<eyre::Result<Vec<String>>>()?,
        })
        .send()
        .await
        .wrap_err("Failed to send request to /envs/list endpoint")?;

    if !response.status().is_success() {
        return Err(ApiError::from_response(response).await?.into());
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
    let functions = project.parse(&[])?;
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
