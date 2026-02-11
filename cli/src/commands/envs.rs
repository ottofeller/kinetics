pub mod list;

use crate::commands::envs::list::ListCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum EnvsCommands {
    /// List all environment variables for all functions
    List(ListCommand),
}
