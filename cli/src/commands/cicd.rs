pub mod github;
pub mod init;

use crate::commands::cicd::init::InitCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum CicdCommands {
    /// Initialize a CI/CD pipeline
    Init(InitCommand),
}
