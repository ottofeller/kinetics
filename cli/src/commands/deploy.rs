use super::build::{pipeline::Pipeline, prepare_functions};
use crate::client::Client;
use crate::config::build_config;
use crate::function::Function;
use crate::project::Project;
use async_trait::async_trait;
use eyre::WrapErr;
use reqwest::StatusCode;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// The entry point to run the command
pub async fn run(
    deploy_functions: &[String],
    max_concurrency: &usize,
    is_only_envs: bool,
    is_hotswap: bool,
    deploy_config: Option<Arc<dyn DeployConfig>>,
) -> eyre::Result<()> {
    if is_only_envs {
        envs(deploy_functions).await?;
    } else {
        full(deploy_functions, *max_concurrency, is_hotswap, deploy_config).await?;
    }

    Ok(())
}

/// Deploy only environment variables for functions
async fn envs(deploy_functions: &[String]) -> eyre::Result<()> {
    println!("{}...", console::style("Provisioning envs").green().bold());
    let project = Project::from_current_dir()?;

    let functions: Vec<Function> = prepare_functions(
        PathBuf::from(build_config()?.kinetics_path),
        &project,
        deploy_functions,
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

        envs.insert(function.name.clone(), function_envs);
    }

    let client = Client::new(false).await?;

    let result = client
        .post("/stack/deploy/envs")
        .json(&json!({
            "project_name": project.name.clone(),
            "functions": envs,
        }))
        .send()
        .await
        .wrap_err("Request to update envs failed")?;

    let status = result.status();
    let response_text = result.text().await?;
    log::debug!("Got status from /stack/deploy/envs: {}", status);
    log::debug!("Got response from /stack/deploy/envs: {}", response_text);

    let response_json: serde_json::Value = serde_json::from_str(&response_text)
        .wrap_err("Failed to parse response from the backend as JSON")?;

    if StatusCode::OK != status {
        log::error!("Got error response: {}", response_text);
        return Err(eyre::eyre!("Failed to deploy envs"));
    }

    let default = &vec![];

    let fails = response_json
        .get("fails")
        .and_then(|v| v.as_array())
        .unwrap_or(default);

    if !fails.is_empty() {
        return Err(eyre::eyre!(
            "Failed to provision envs for: {}",
            fails
                .iter()
                .map(|v| v.as_str().unwrap_or("unknown"))
                .collect::<Vec<&str>>()
                .join(", "),
        ));
    }

    println!("{}", console::style("Done").green().bold(),);
    Ok(())
}

/// Do full deployment of requested functions
async fn full(
    deploy_functions: &[String],
    max_concurrency: usize,
    is_hotswap: bool,
    deploy_config: Option<Arc<dyn DeployConfig>>,
) -> eyre::Result<()> {
    Pipeline::builder()
        .set_max_concurrent(max_concurrency)
        .with_deploy_enabled(true)
        .with_hotswap(is_hotswap)
        .with_deploy_config(deploy_config)
        .set_project(Project::from_current_dir()?)
        .build()
        .wrap_err("Failed to build pipeline")?
        .run(deploy_functions)
        .await
}

#[async_trait]
pub trait DeployConfig: Send + Sync {
    async fn deploy(
        &self,
        project: &Project,
        secrets: HashMap<String, String>,
        functions: &[Function],
    ) -> eyre::Result<bool>;
    async fn upload(&self, function: &mut Function) -> eyre::Result<bool>;
}
