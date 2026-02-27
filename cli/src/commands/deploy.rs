mod runner;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use clap::ArgAction;
use runner::DeployRunner;

#[derive(clap::Args, Clone)]
pub(crate) struct DeployCommand {
    /// Maximum number of parallel concurrent builds
    #[arg(short, long, default_value_t = 3)]
    max_concurrency: usize,

    /// Deploy only environment variables instead of full deployment
    #[arg(short, long, action = ArgAction::SetTrue)]
    envs: bool,

    /// Use hotswap deployment for faster updates
    #[arg(long, action = ArgAction::SetTrue)]
    hotswap: bool,

    /// The set of functions to deploy, comma separated
    #[arg(value_delimiter = ',')]
    functions: Vec<String>,
}

impl Runnable for DeployCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        DeployRunner {
            command: self.clone(),
            writer,
        }
    }
}
