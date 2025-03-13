use crate::client::Client;
use crate::crat::Crate;
use crate::function::Function;
use eyre::{eyre, Context, OptionExt, Report};
use futures::future;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::{Arc};
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::time::Duration;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone)]
pub struct Pipeline {
    is_deploy_enabled: bool,
    is_directly: bool,
    crat: Crate,
    max_concurrent: usize,
}

impl Pipeline {
    pub fn builder() -> PipelineBuilder {
        PipelineBuilder::default()
    }

    pub async fn run(self, functions: Vec<Function>) -> eyre::Result<()> {
        // Define maximum number of parallel builds
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent));

        let start_time = Instant::now();
        let pipeline_progress = PipelineProgress::new(functions.len() as u64);

        let client = if self.is_deploy_enabled {
            Some(Client::new(&self.is_directly).wrap_err("Failed to create client")?)
        } else {
            None
        };

        let handles = functions.into_iter().map(|mut function| {
            let client = client.clone();
            let sem = Arc::clone(&semaphore);

            let pipeline_progress = pipeline_progress.clone();

            tokio::spawn(async move {
                // Acquire permit before sending request.
                let _permit = sem.acquire().await?;

                let function_name = function.name()?;

                let function_progress = pipeline_progress.new_progress(&function_name);
                function_progress.log_stage("Building ");

                function.build().await.map_err(|e| {
                    function_progress.error();
                    e.wrap_err(eyre!("Failed to build function: \"{}\"", function_name))
                })?;

                if !self.is_deploy_enabled {
                    function_progress.done();
                    pipeline_progress.increase_current_function_position();
                    return Ok(function);
                }

                function_progress.log_stage("Bundling ");

                function.bundle().await.map_err(|e| {
                    function_progress.error();
                    e.wrap_err(eyre!("Failed to bundle function: \"{}\"", function_name))
                })?;

                function_progress.log_stage("Uploading");

                crate::deploy::upload(
                    &client.ok_or_eyre("Client must be initialized when deployment is enabled")?,
                    &mut function,
                    &self.is_directly,
                )
                .await
                .map_err(|e| {
                    function_progress.error();
                    e.wrap_err(eyre!("Failed to upload function: \"{}\"", function_name))
                })?;

                function_progress.done();
                pipeline_progress.increase_current_function_position();

                if let Err(error) = tokio::fs::remove_file(function.bundle_path()).await {
                    eprintln!(
                        "Failed to remove file {:?} with error {}",
                        function.bundle_path(),
                        error,
                    );
                };

                Ok::<Function, Report>(function)
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

        let (mut ok_results, errors): (Vec<_>, Vec<_>) =
            results.into_iter().partition(Result::is_ok);

        if !errors.is_empty() {
            return Err(eyre!(
                "Failed to process function(s): {:?}",
                errors
                    .into_iter()
                    .map(Result::unwrap_err)
                    .collect::<Vec<_>>()
            ));
        }

        pipeline_progress.total_progress_bar.finish_and_clear();

        if !self.is_deploy_enabled {
            println!(
                "{} \"{}\" building and uploading in {:.2}s",
                console::style("Finished").green().bold(),
                self.crat.name,
                start_time.elapsed().as_secs_f64(),
            );

            return Ok(());
        }

        let deploying_progress = pipeline_progress.new_progress(&self.crat.name);

        deploying_progress.log_stage("Deploying");

        // It's safe to unwrap here because the errors have already been caught
        let functions: Vec<_> = ok_results.drain(..).map(Result::unwrap).collect();

        crate::deploy::deploy(&self.crat, &functions, &self.is_directly)
            .await
            .wrap_err(eyre!("Failed to deploy functions"))?;

        deploying_progress.progress_bar.finish_and_clear();

        println!(
            "{} \"{}\" has been successfully deployed in {:.2}s",
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
    is_directly: Option<bool>,
    crat: Option<Crate>,
    max_concurrent: Option<usize>,
}

impl PipelineBuilder {
    pub fn build(self) -> eyre::Result<Pipeline> {
        if self.crat.is_none() {
            return Err(eyre!("No crate provided to the pipeline"));
        }

        Ok(Pipeline {
            crat: self.crat.unwrap(),
            is_deploy_enabled: self.is_deploy_enabled.unwrap_or(false),
            is_directly: self.is_directly.unwrap_or(false),
            max_concurrent: self.max_concurrent.unwrap_or(4),
        })
    }

    pub fn with_deploy_enabled(mut self, is_deploy_enabled: bool) -> Self {
        self.is_deploy_enabled = Some(is_deploy_enabled);
        self
    }

    pub fn with_directly(mut self, is_directly: bool) -> Self {
        self.is_directly = Some(is_directly);
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
    fn new(total_functions: u64) -> Self {
        let multi_progress = MultiProgress::new();
        let completed_functions_count = Arc::new(AtomicUsize::new(0));
        let total_progress_bar = multi_progress.add(ProgressBar::new(total_functions));

        total_progress_bar.set_style(
            ProgressStyle::default_bar()
                .template("[{pos}/{len}] functions uploaded [{bar:40}] {percent}%")
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
        let count = self.completed_functions_count.fetch_add(1, Ordering::SeqCst) + 1;
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

struct Progress {
    progress_bar: ProgressBar,
    resource_name: String,
}

impl Progress {
    fn new(
        multi_progress: &MultiProgress,
        total_progress_bar: &ProgressBar,
        function_name: &str,
    ) -> Self {
        let function_progress_bar =
            multi_progress.insert_before(&total_progress_bar, ProgressBar::new_spinner());

        function_progress_bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
        );

        function_progress_bar.enable_steady_tick(Duration::from_millis(50));

        Self {
            progress_bar: function_progress_bar,
            resource_name: function_name.to_string(),
        }
    }

    fn log_stage(&self, stage: &str) {
        self.progress_bar.set_message(format!(
            "{} {}",
            console::style(stage).green().bold(),
            self.resource_name,
        ));
    }

    fn done(&self) {
        self.progress_bar.finish_with_message(format!(
            "{}      {}",
            console::style("Done").green().bold(),
            self.resource_name,
        ));
    }

    fn error(&self) {
        self.progress_bar.finish_with_message(format!(
            "{}     {}",
            console::style("Error").red().bold(),
            self.resource_name,
        ));
    }
}
