use crate::logger::Logger;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::io::{stdout, IsTerminal};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct PipelineProgress<'a> {
    multi_progress: &'a MultiProgress,
    pub(super) total_progress_bar: ProgressBar,
    completed_functions_count: Arc<AtomicUsize>,
}

impl<'a> PipelineProgress<'a> {
    pub(super) fn new(total_functions: u64, is_deploy: bool) -> Self {
        let multi_progress = Logger::multi_progress();
        let completed_functions_count = Arc::new(AtomicUsize::new(0));

        // +1 for provisioning phase
        // +1 for build phase
        let total_progress_bar = multi_progress.add(ProgressBar::new(total_functions + 2));

        total_progress_bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    format!(
                        "   {} [{{bar:30}}] {{pos}}/{{len}} {{wide_msg:.dim}}",
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

    pub(super) fn increase_current_function_position(&self) {
        self.completed_functions_count
            .fetch_add(1, Ordering::SeqCst);

        self.total_progress_bar
            .set_position(self.completed_functions_count.load(Ordering::Relaxed) as u64);
    }

    pub(super) fn new_progress(&self, resource_name: &str) -> Progress {
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

pub(super) enum ProgressStatus {
    #[allow(dead_code)]
    Success,
    Warn,
    Error,
}

impl Progress {
    fn new(
        multi_progress: &MultiProgress,
        total_progress_bar: &ProgressBar,
        function_name: &str,
    ) -> Self {
        let function_progress_bar =
            multi_progress.insert_before(total_progress_bar, ProgressBar::new_spinner());

        function_progress_bar.set_style(ProgressStyle::with_template("{msg}").unwrap());

        Self {
            progress_bar: function_progress_bar,
            resource_name: function_name.to_string(),
        }
    }

    pub(super) fn log_stage(&self, stage: &str) {
        let msg = format!(
            "{} {}",
            console::style(self.with_padding(stage)).green().bold(),
            self.resource_name,
        );

        // Terminal or CI/CD?
        if stdout().is_terminal() {
            self.progress_bar.println(msg);
        } else {
            self.progress_bar.suspend(|| {
                println!("{msg}");
            });
        }
    }

    pub(super) fn finish(&self, stage: &str, status: ProgressStatus, message: Option<&str>) {
        let stage = console::style(self.with_padding(stage)).bold();
        let stage = match status {
            ProgressStatus::Success => stage.green(),
            ProgressStatus::Warn => stage.yellow(),
            ProgressStatus::Error => stage.red(),
        };
        let message = message.map(|m| format!(": {m}")).unwrap_or_default();
        self.progress_bar
            .finish_with_message(format!("{} {}{}", stage, self.resource_name, message));
    }

    pub(super) fn error(&self, stage: &str) {
        self.finish(stage, ProgressStatus::Error, None);
    }

    // Required padding to make the message centered in the cargo-like style
    fn with_padding(&self, message: &str) -> String {
        let len = message.len();
        let padding = " ".repeat(12 - len);
        format!("{}{}", padding, message)
    }
}
