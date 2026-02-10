pub(crate) mod pipeline;
pub mod progress;
mod runner;
use crate::runner::{Runnable, Runner};
use runner::BuildRunner;

#[derive(clap::Args, Clone)]
pub(crate) struct BuildCommand {
    /// Comma-separated list of function names to build (if not specified, all functions will be built)
    #[arg(short, long, value_delimiter = ',')]
    pub(crate) functions: Vec<String>,
}

impl Runnable for BuildCommand {
    fn runner(&self) -> impl Runner {
        BuildRunner {
            command: self.clone(),
        }
    }
}
