use crate::api::stack;
use crate::commands::build::{pipeline::Pipeline, prepare_functions};
use crate::commands::deploy::DeployCommand;
use crate::config::build_config;
use crate::error::Error;
use crate::function::Function;
use crate::project::Project;
use crate::runner::Runner;
use eyre::Context;
use reqwest::StatusCode;
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) struct DeployRunner {
    pub(crate) command: DeployCommand,
}

impl Runner for DeployRunner {
    /// Invoke the function either locally or remotely
    async fn run(&mut self) -> Result<(), Error> {
        if self.command.envs {
            self.deploy_envs().await?;
        } else {
            self.deploy_all().await?;
        }

        Ok(())
    }
}

impl DeployRunner {
    /// Deploy only environment variables for functions
    async fn deploy_envs(&mut self) -> eyre::Result<()> {
        println!("{}...", console::style("Provisioning envs").green().bold());
        let client = self.api_client().await?;
        let project = Project::from_current_dir()?;

        let functions: Vec<Function> = prepare_functions(
            PathBuf::from(build_config()?.kinetics_path),
            &project,
            &self.command.functions,
        )?
        .iter()
        .filter(|f| f.is_deploying)
        .cloned()
        .collect();

        if functions.is_empty() {
            println!("{}", console::style("No functions found").yellow().bold(),);
            return Ok(());
        }

        // Collect environment variables from all functions
        // {"<Function name>": {"<Env>": "<Value>"}}
        let mut envs = HashMap::new();

        for function in &functions {
            let function_envs = function.environment();

            let envs_string = function_envs
                .keys()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", ");

            println!(
                "{} {}",
                console::style(function.name.clone()).bold(),
                if envs_string.is_empty() {
                    console::style("None").dim().yellow()
                } else {
                    console::style(envs_string.as_str()).dim()
                }
            );

            envs.insert(function.name.clone(), function_envs.clone());
        }

        let result = client
            .post("/stack/deploy/envs")
            .json(&stack::deploy::envs::Request {
                project_name: project.name.clone(),
                functions: envs,
            })
            .send()
            .await
            .wrap_err("Request to update envs failed")?;

        let status = result.status();
        let response_text = result.text().await?;
        log::debug!("Got status from /stack/deploy/envs: {}", status);
        log::debug!("Got response from /stack/deploy/envs: {}", response_text);

        let response_json: stack::deploy::envs::Response = serde_json::from_str(&response_text)
            .wrap_err("Failed to parse response from the backend as JSON")?;

        if StatusCode::OK != status {
            log::error!("Got error response: {}", response_text);
            return Err(eyre::eyre!("Failed to deploy envs"));
        }

        let fails = response_json.fails;

        if !fails.is_empty() {
            return Err(eyre::eyre!(
                "Failed to provision envs for: {}",
                fails
                    .iter()
                    .map(|v| v.as_str())
                    .collect::<Vec<&str>>()
                    .join(", "),
            ));
        }

        println!("{}", console::style("Done").green().bold(),);
        Ok(())
    }

    /// Do full deployment of requested functions
    async fn deploy_all(&self) -> eyre::Result<()> {
        Pipeline::builder()
            .set_max_concurrent(self.command.max_concurrency)
            .with_deploy_enabled(true)
            .with_hotswap(self.command.hotswap)
            .set_project(Project::from_current_dir()?)
            .build()
            .wrap_err("Failed to build pipeline")?
            .run(&self.command.functions)
            .await
    }
}
