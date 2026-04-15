mod add;
mod remove;
mod status;

use crate::commands::domain::add::AddCommand;
use crate::commands::domain::remove::RemoveCommand;
use crate::commands::domain::status::StatusCommand;
use clap::Subcommand;

/// Manage custom domains for the project
#[derive(Subcommand)]
pub(crate) enum DomainCommands {
    /// Link a custom domain and provision DNS records
    Add(AddCommand),
    /// Show current domain verification status
    Status(StatusCommand),
    /// Detach the domain from the project
    Remove(RemoveCommand),
}
