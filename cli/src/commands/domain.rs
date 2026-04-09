mod add;
mod remove;
mod status;

use crate::commands::domain::add::AddCommand;
use crate::commands::domain::remove::RemoveCommand;
use crate::commands::domain::status::StatusCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum DomainCommands {
    Add(AddCommand),
    Status(StatusCommand),
    Remove(RemoveCommand),
}
