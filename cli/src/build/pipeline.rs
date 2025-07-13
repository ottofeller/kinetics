use crate::client::Client;
use crate::crat::Crate;
use crate::deploy::DeployConfig;
use crate::function::{build, Function};
use eyre::{eyre, OptionExt, Report};
use futures::future;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;

pub struct Pipeline {
    is_deploy_enabled: bool,
    crat: Crate,
    max_concurrent: usize,
    deploy_config: Option<Arc<dyn DeployConfig>>,
}

impl Pipeline {
    pub fn builder() -> PipelineBuilder {
        PipelineBuilder::default()
    }

    pub async fn run(
        self,

        // All functions are sent to server, so that related resources are always prepared
        all_functions: &[Function],

        // Only selected functions are built and uploaded
        deploy_functions: &[String],
    ) -> eyre::Result<()> {
        if self.deploy_config.is_some() {
            println!(
                "    {} `{}` {}",
                console::style("Using a custom deployment configuration for").yellow(),
                console::style(&self.crat.name).green().bold(),
                console::style("crate").yellow(),
            );
        }

        let deploy_functions: Vec<Function> = if deploy_functions.is_empty() {
            all_functions.iter().cloned().collect()
        } else {
            all_functions
                .iter()
                .cloned()
                .filter(|f| deploy_functions.contains(&f.name))
                .collect()
        };

        let start_time = Instant::now();

        let pipeline_progress = PipelineProgress::new(
            1 // One for entire crate build
            + deploy_functions.len() as u64 * if self.is_deploy_enabled {
                2
            } else {
                0
            },
            self.is_deploy_enabled,
        );

        let client = if self.is_deploy_enabled {
            Some(Client::new(self.deploy_config.is_some())?)
        } else {
            None
        };

        let deploying_progress = pipeline_progress.new_progress(&self.crat.name);
        build(&deploy_functions, &deploying_progress).await?;
        pipeline_progress.increase_current_function_position();

        if !self.is_deploy_enabled {
            pipeline_progress.increase_current_function_position();
            pipeline_progress.total_progress_bar.finish_and_clear();

            println!(
                "    {} `{}` crate building in {:.2}s",
                console::style("Finished").green().bold(),
                self.crat.name,
                start_time.elapsed().as_secs_f64(),
            );

            return Ok(());
        }

        // Define maximum number of parallel bundling jobs
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent));

        let handles = deploy_functions.into_iter().map(|mut function| {
            let client = client.clone();
            let sem = Arc::clone(&semaphore);
            let deploy_config_clone = self.deploy_config.clone();
            let pipeline_progress = pipeline_progress.clone();

            tokio::spawn(async move {
                // Acquire permit before sending request.
                let _permit = sem.acquire().await?;

                let function_progress = pipeline_progress.new_progress(&function.name);
                pipeline_progress.increase_current_function_position();

                function_progress.log_stage("Bundling");

                function.bundle().await.map_err(|e| {
                    function_progress.error("Bundling");
                    e.wrap_err(format!("Failed to bundle function: \"{}\"", function.name))
                })?;

                pipeline_progress.increase_current_function_position();
                function_progress.log_stage("Uploading");

                function
                    .upload(&client.unwrap(), deploy_config_clone.as_deref())
                    .await
                    .map_err(|e| {
                        function_progress.error("Uploading");
                        e.wrap_err(format!("Failed to upload function: \"{}\"", function.name))
                    })?;

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
            return Err(eyre!(
                "Failed to process function(s): {:?}",
                errors
                    .into_iter()
                    .map(Result::unwrap_err)
                    .collect::<Vec<_>>()
            ));
        }

        deploying_progress.log_stage("Provisioning");

        let deploy = self
            .crat
            .deploy(all_functions, self.deploy_config.as_deref())
            .await;

        if deploy.is_err() {
            deploying_progress.error("Provisioning");
            pipeline_progress.total_progress_bar.finish_and_clear();
            return Err(deploy.err().unwrap());
        }

        deploying_progress.progress_bar.finish_and_clear();
        let mut status = self.crat.status().await?;

        // Poll the status of the deployment
        while status.status == "IN_PROGRESS" {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            status = self.crat.status().await?;
        }

        if status.status == "FAILED" {
            deploying_progress.error("Provisioning");
            pipeline_progress.total_progress_bar.finish_and_clear();
            return Err(eyre!("{}", status.errors.unwrap().join("\n")));
        }

        pipeline_progress.increase_current_function_position();
        pipeline_progress.total_progress_bar.finish_and_clear();

        println!(
            "    {} `{}` crate deployed in {:.2}s",
            console::style("Finished").green().bold(),
            self.crat.name,
            start_time.elapsed().as_secs_f64(),
        );

        Ok(())
    }
}

#[derive(Default)]
pub struct PipelineBuilder {
    is_deploy_enabled: Option<bool>,
    crat: Option<Crate>,
    max_concurrent: Option<usize>,
    deploy_config: Option<Arc<dyn DeployConfig>>,
}

impl PipelineBuilder {
    pub fn build(self) -> eyre::Result<Pipeline> {
        Ok(Pipeline {
            crat: self.crat.ok_or_eyre("No crate provided to the pipeline")?,
            is_deploy_enabled: self.is_deploy_enabled.unwrap_or(false),
            max_concurrent: self.max_concurrent.unwrap_or(6),
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

    pub fn set_crat(mut self, crat: Crate) -> Self {
        self.crat = Some(crat);
        self
    }

    pub fn set_max_concurrent(mut self, max_concurrent: usize) -> Self {
        self.max_concurrent = Some(max_concurrent);
        self
    }
}

#[derive(Clone)]
struct PipelineProgress {
    multi_progress: MultiProgress,
    total_progress_bar: ProgressBar,
    completed_functions_count: Arc<AtomicUsize>,
}

impl PipelineProgress {
    fn new(total_functions: u64, is_deploy: bool) -> Self {
        let multi_progress = MultiProgress::new();
        let completed_functions_count = Arc::new(AtomicUsize::new(0));

        // +1 for provisioning phase
        let total_progress_bar = multi_progress.add(ProgressBar::new(total_functions + 1));

        total_progress_bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    format!(
                        "   {} [{{bar:40}}] {{percent}}%",
                        console::style(if is_deploy { "Deploying" } else { "Building" })
                            .cyan()
                            .bold()
                    )
                    .as_str(),
                )
                .unwrap()
                .progress_chars("=> "),
        );

        total_progress_bar.set_position(0);

        Self {
            multi_progress,
            total_progress_bar,
            completed_functions_count,
        }
    }

    fn increase_current_function_position(&self) {
        let count = self
            .completed_functions_count
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        self.total_progress_bar.set_position(count as u64);
    }

    fn new_progress(&self, resource_name: &str) -> Progress {
        Progress::new(
            &self.multi_progress,
            &self.total_progress_bar,
            resource_name,
        )
    }
}

pub struct Progress {
    pub progress_bar: ProgressBar,
    resource_name: String,
}

impl Progress {
    fn new(
        multi_progress: &MultiProgress,
        total_progress_bar: &ProgressBar,
        function_name: &str,
    ) -> Self {
        let function_progress_bar =
            multi_progress.insert_before(total_progress_bar, ProgressBar::new_spinner());

        function_progress_bar
            .set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());

        Self {
            progress_bar: function_progress_bar,
            resource_name: function_name.to_string(),
        }
    }

    fn log_stage(&self, stage: &str) {
        self.progress_bar.println(format!(
            "{} {}",
            console::style(self.with_padding(stage)).green().bold(),
            self.resource_name,
        ));
    }

    fn error(&self, stage: &str) {
        self.progress_bar.finish_with_message(format!(
            "{} {}",
            console::style(self.with_padding(stage)).red().bold(),
            self.resource_name,
        ));
    }

    // Required padding to make the message centered in the cargo-like style
    fn with_padding(&self, message: &str) -> String {
        let len = message.len();
        let padding = " ".repeat(12 - len);
        format!("{}{}", padding, message)
    }
}
