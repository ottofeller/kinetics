pub mod add;
mod config;
pub mod remove;

use crate::commands::domains::add::AddCommand;
use crate::commands::domains::remove::RemoveCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum DomainsCommands {
    /// Attach a custom domain to the project
    Add(AddCommand),

    /// Remove the custom domain from the project
    Remove(RemoveCommand),
}
