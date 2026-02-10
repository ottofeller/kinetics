use super::progress::{PipelineProgress, ProgressStatus};
use crate::api::client::Client;
use crate::config::build_config;
use crate::config::deploy::DeployConfig;
use crate::function::{build, Function};
use crate::project::Project;
use eyre::{eyre, OptionExt, Report};
use futures::future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;

pub struct Pipeline {
    is_deploy_enabled: bool,
    is_hotswap: bool,
    project: Project,
    max_concurrent: usize,
    deploy_config: Option<Arc<dyn DeployConfig>>,
}

impl Pipeline {
    pub fn builder() -> PipelineBuilder {
        PipelineBuilder::default()
    }

    pub async fn run(
        self,

        // Only selected functions are built and uploaded
        deploy_functions: &[String],
    ) -> eyre::Result<()> {
        if self.deploy_config.is_some() {
            println!(
                "    {} `{}` {}",
                console::style("Using a custom deployment configuration for").yellow(),
                console::style(&self.project.name).green().bold(),
                console::style("project").yellow(),
            );
        }

        let start_time = Instant::now();
        print!("{}...", console::style("Preparing").green().bold(),);

        // All functions to add to the template
        let all_functions = self.project.parse(
            PathBuf::from(build_config()?.kinetics_path),
            deploy_functions,
        )?;

        // Clear the previous line, the "Preparing..." step is not a part of the build pipeline
        print!("\r\x1B[K");

        let deploy_functions: Vec<Function> = all_functions
            .iter()
            .filter(|f| f.is_deploying)
            .cloned()
            .collect();

        let pipeline_progress = PipelineProgress::new(
            deploy_functions.len() as u64 * if self.is_deploy_enabled { 1 } else { 0 },
            self.is_deploy_enabled,
        );

        let deploying_progress = pipeline_progress.new_progress(&self.project.name);

        pipeline_progress
            .new_progress(&self.project.name)
            .log_stage("Building");

        build(&deploy_functions, &pipeline_progress.total_progress_bar).await?;
        pipeline_progress.increase_current_function_position();

        if !self.is_deploy_enabled {
            pipeline_progress.increase_current_function_position();
            pipeline_progress.total_progress_bar.finish_and_clear();

            println!(
                "    {} `{}` project building in {:.2}s",
                console::style("Finished").green().bold(),
                self.project.name,
                start_time.elapsed().as_secs_f64(),
            );

            return Ok(());
        }

        // Define maximum number of parallel bundling jobs
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent));

        let deploy_functions_len = deploy_functions.len();

        let client = Client::new(self.deploy_config.is_some()).await?;

        let handles = deploy_functions.into_iter().map(|mut function| {
            let client = client.clone();
            let sem = Arc::clone(&semaphore);
            let deploy_config_clone = self.deploy_config.clone();
            let pipeline_progress = pipeline_progress.clone();

            tokio::spawn(async move {
                // Acquire permit before sending request.
                let _permit = sem.acquire().await?;

                let function_progress = pipeline_progress.new_progress(&function.name);
                function_progress.log_stage("Uploading");

                match function
                    .upload(&client, deploy_config_clone.as_deref())
                    .await
                {
                    Ok(updated) => {
                        if !updated {
                            function_progress.finish(
                                "Uploading",
                                ProgressStatus::Warn,
                                Some("No changes, skipped"),
                            );
                        }
                        Ok(())
                    }
                    Err(e) => {
                        function_progress.error("Uploading");
                        Err(e.wrap_err(format!("Failed to upload function: \"{}\"", function.name)))
                    }
                }?;

                pipeline_progress.increase_current_function_position();

                if let Err(error) = tokio::fs::remove_file(function.bundle_path()).await {
                    eprintln!(
                        "Failed to remove file {:?} with error {}",
                        function.bundle_path(),
                        error,
                    );
                };

                Ok(())
            })
        });

        let results: Vec<_> = future::join_all(handles)
            .await
            .into_iter()
            .map(|res| {
                res.map_err(Report::msg)
                    .and_then(|inner_result| inner_result)
            })
            .collect();

        let (.., errors): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);

        if !errors.is_empty() {
            log::error!(
                "Failed to process functions: {:?}",
                errors
                    .into_iter()
                    .map(Result::unwrap_err)
                    .collect::<Vec<_>>()
            );

            return Err(eyre!("Failed to process function(s)"));
        }

        // Check if there's an ongoing deployment and wait for it to finish
        let mut status = self.project.status().await?;
        log::debug!("Pipeline status: {:?}", status.status);
        deploying_progress.log_stage("Provisioning");

        if status.status == "IN_PROGRESS" {
            pipeline_progress
                .total_progress_bar
                .set_message("Waiting for previous deployment to finish...");
        }

        while status.status == "IN_PROGRESS" {
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            status = self.project.status().await?;
        }

        pipeline_progress.total_progress_bar.set_message(
            if deploy_functions_len >= build_config()?.provision_warn_threshold {
                "May take longer than a minute..."
            } else {
                "Provisioning resources..."
            },
        );

        match self
            .project
            .deploy(
                &all_functions,
                self.is_hotswap,
                self.deploy_config.as_deref(),
            )
            .await
        {
            Ok(updated) if !updated => {
                deploying_progress.finish(
                    "Provisioning",
                    ProgressStatus::Warn,
                    Some("Nothing to update"),
                );
            }
            Ok(_) => {
                // Wait for stack deployment if it is updated.
                deploying_progress.progress_bar.finish_and_clear();
                let mut status = self.project.status().await?;

                // Poll the status of the deployment
                while status.status == "IN_PROGRESS" {
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                    status = self.project.status().await?;
                }

                if status.status == "FAILED" {
                    deploying_progress.error("Provisioning");
                    pipeline_progress.total_progress_bar.finish_and_clear();
                    return Err(eyre!("{}", status.errors.unwrap().join("\n")));
                }
            }
            Err(err) => {
                deploying_progress.error("Provisioning");
                pipeline_progress.total_progress_bar.finish_and_clear();
                return Err(err);
            }
        };

        pipeline_progress.increase_current_function_position();
        pipeline_progress.total_progress_bar.finish_and_clear();

        println!(
            "    {} Deployed in {:.2}s",
            console::style("Finished").green().bold(),
            start_time.elapsed().as_secs_f64(),
        );

        Ok(())
    }
}

#[derive(Default)]
pub struct PipelineBuilder {
    is_deploy_enabled: Option<bool>,
    is_hotswap: Option<bool>,
    project: Option<Project>,
    max_concurrent: Option<usize>,
    deploy_config: Option<Arc<dyn DeployConfig>>,
}

impl PipelineBuilder {
    pub fn build(self) -> eyre::Result<Pipeline> {
        Ok(Pipeline {
            project: self
                .project
                .ok_or_eyre("No project provided to the pipeline")?,
            is_deploy_enabled: self.is_deploy_enabled.unwrap_or(false),
            is_hotswap: self.is_hotswap.unwrap_or(false),
            max_concurrent: self.max_concurrent.unwrap_or(10),
            deploy_config: self.deploy_config,
        })
    }

    pub fn with_deploy_config(mut self, config: Option<Arc<dyn DeployConfig>>) -> Self {
        self.deploy_config = config;
        self
    }

    pub fn with_deploy_enabled(mut self, is_deploy_enabled: bool) -> Self {
        self.is_deploy_enabled = Some(is_deploy_enabled);
        self
    }

    pub fn with_hotswap(mut self, is_hotswap: bool) -> Self {
        self.is_hotswap = Some(is_hotswap);
        self
    }

    pub fn set_project(mut self, project: Project) -> Self {
        self.project = Some(project);
        self
    }

    pub fn set_max_concurrent(mut self, max_concurrent: usize) -> Self {
        self.max_concurrent = Some(max_concurrent);
        self
    }
}
