pub mod create;
use crate::commands::orgs::create::CreateCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum OrgsCommands {
    /// Create new org with you as an owner
    Create(CreateCommand),
}
