use crate::api::error::ApiError;
use crate::build;
use crate::commands::build::pipeline::Pipeline;
use crate::commands::deploy::DeployCommand;
use crate::commands::envs::check_results;
use crate::error::Error;
use crate::function::Function;
use crate::project::Project;
use crate::runner::Runner;
use crate::writer::Writer;
use eyre::Context;
use kinetics_api::request::Validate;
use kinetics_api::stack;
use serde_json::json;
use std::collections::HashMap;

pub(crate) struct DeployRunner<'a> {
    pub(crate) command: DeployCommand,
    pub(crate) writer: &'a Writer,
}

impl Runner for DeployRunner<'_> {
    /// Invoke the function either locally or remotely
    async fn run(&mut self) -> Result<(), Error> {
        let mut project = self.project(&self.command.project).await?;

        if self.command.org.is_some() {
            project = project.with_org(self.command.org.as_deref());
        }

        // DataDog API key only needed during deployment, to send it to the backend
        if project
            .observability
            .as_ref()
            .filter(|o| o.dd_api_key.is_empty())
            .is_some()
        {
            return Err(Error::new(
                "DataDog API key is missing",
                Some("Failed to read from specified env var. Check kinetics.toml:[observability]"),
            ));
        }

        let envs = if self.command.envs {
            Some(self.deploy_envs(project).await?)
        } else {
            self.deploy_all(project).await?;
            None
        };

        Project::clear_cache().map_err(|e| {
            self.error(
                Some("Failed to clear project cache"),
                Some(&e.to_string()),
                Some(e.into()),
            )
        })?;

        match envs {
            Some(response) => self
                .writer
                .json(json!({"success": true, "results": response.results}))?,
            None => self.writer.json(json!({"success": true}))?,
        }
        Ok(())
    }
}

impl DeployRunner<'_> {
    /// Deploy only environment variables for functions
    async fn deploy_envs(
        &mut self,
        project: Project,
    ) -> eyre::Result<stack::deploy::envs::Response> {
        self.writer.text(&format!(
            "{} envs...\n",
            console::style("Provisioning").bold()
        ))?;

        let functions: Vec<Function> = build::generate_workspace(
            &project,
            &self.command.functions,
            self.command.package.as_deref(),
            &project.build_path()?,
        )?
        .iter()
        .filter(|f| f.is_deploying)
        .cloned()
        .collect();

        if functions.is_empty() {
            self.writer.text(&format!(
                "{}\n",
                console::style("No functions found").yellow()
            ))?;

            return Ok(stack::deploy::envs::Response {
                results: Default::default(),
            });
        }

        let client = self.api_client().await?;

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

            self.writer.text(&format!(
                "{} {}\n",
                function.name,
                if envs_string.is_empty() {
                    console::style("None").dim().yellow()
                } else {
                    console::style(envs_string.as_str()).dim()
                }
            ))?;

            envs.insert(function.name.clone(), function_envs.clone());
        }

        let request = stack::deploy::envs::Request {
            project: (&project).into(),
            functions: envs,
        };
        if let Some(errors) = request.validate() {
            return Err(eyre::eyre!("{}", errors.join("\n")));
        }

        let result = client
            .post("/stack/deploy/envs")
            .json(&request)
            .send()
            .await
            .wrap_err("Request to update envs failed")?;

        if !result.status().is_success() {
            return Err(ApiError::from_response(result).await?.into());
        }

        let response: stack::deploy::envs::Response = result.json().await.wrap_err(Error::new(
            "Invalid response from server",
            Some("Try again later."),
        ))?;

        for (name, result) in &response.results {
            if matches!(result, kinetics_api::envs::FunctionResult::Success { .. }) {
                self.writer
                    .text(&format!("{name}: environment update accepted\n"))?;
            }
        }

        check_results(&response, "Failed to update environment variables")?;

        self.writer
            .text(&format!("{}\n", console::style("Done").bold()))?;

        Ok(response)
    }

    /// Do full deployment of requested functions
    async fn deploy_all(&self, project: Project) -> Result<(), Error> {
        Pipeline::builder(self.writer)
            .set_max_concurrent(self.command.max_concurrency)
            .with_deploy_enabled(true)
            .with_hotswap(self.command.hotswap)
            .with_package(self.command.package.clone())
            .with_version_message(self.command.message.clone())
            .set_project(project)
            .build()
            .wrap_err("Failed to build pipeline")?
            .run(&self.command.functions)
            .await
    }
}
