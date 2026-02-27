mod runner;

use std::path::PathBuf;

use crate::{runner::{Runnable, Runner}, writer::Writer};
use runner::InitRunner;

#[derive(clap::Args, Clone)]
pub(crate) struct InitCommand {
    /// Name of the project to create
    #[arg()]
    pub(crate) name: String,

    /// Cron job template
    #[arg(short, long, action = clap::ArgAction::SetTrue, required = false)]
    pub(crate) cron: bool,

    /// REST API endpoint
    #[arg(short, long, action = clap::ArgAction::SetTrue, required = false)]
    pub(crate) endpoint: bool,

    /// Queue worker
    #[arg(short, long, action = clap::ArgAction::SetTrue, required = false)]
    pub(crate) worker: bool,

    /// Disable git repository initialization
    #[arg(short, long)]
    pub(crate) no_git: bool,
}

impl Runnable for InitCommand {
    fn runner(&self, _writer: &Writer) -> impl Runner {
        InitRunner {
            command: self.clone(),
            dir: PathBuf::default(),
        }
    }
}
