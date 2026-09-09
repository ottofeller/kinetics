pub(crate) mod pipeline;
pub mod progress;
mod runner;
use crate::{
    runner::{Runnable, Runner},
    writer::Writer,
};
use runner::BuildRunner;
use std::path::PathBuf;

#[derive(clap::Args, Clone)]
pub(crate) struct BuildCommand {
    /// Comma-separated list of function names to build (if not specified, all functions will be built)
    #[arg(short, long, value_delimiter = ',')]
    pub(crate) functions: Vec<String>,

    /// Build only functions from the workspace package with this Cargo package name
    #[arg(long)]
    pub(crate) package: Option<String>,

    /// Relative path to the project directory
    #[arg(long)]
    pub(crate) project: Option<PathBuf>,
}

impl Runnable for BuildCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        BuildRunner {
            command: self.clone(),
            writer,
        }
    }
}
