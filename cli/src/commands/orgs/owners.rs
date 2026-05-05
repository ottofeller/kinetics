pub mod add;

use crate::commands::orgs::owners::add::AddOwnerCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum OwnersCommands {
    /// Add a new owner to an org
    Add(AddOwnerCommand),
}
