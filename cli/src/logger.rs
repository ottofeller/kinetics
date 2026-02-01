use std::sync::OnceLock;

use indicatif::MultiProgress;
use indicatif_log_bridge::LogWrapper;

/// Set up log levels, formatting, and other configurations for the logger
pub struct Logger {
    multi_progress: MultiProgress,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

impl<'a> Logger {
    pub fn init() -> &'a Self {
        LOGGER.get_or_init(|| {
            let logger = env_logger::Builder::from_env(
                // No logs shown by default, only human-friendly messages
                // Enable logs output with "export RUST_LOG=error" in terminal
                env_logger::Env::default().default_filter_or("off"),
            )
            .build();

            let level = logger.filter();
            let multi_progress = MultiProgress::new();

            LogWrapper::new(multi_progress.clone(), logger)
                .try_init()
                .unwrap();
            log::set_max_level(level);

            Self { multi_progress }
        })
    }

    pub fn multi_progress() -> &'a MultiProgress {
        &Self::init().multi_progress
    }
}
