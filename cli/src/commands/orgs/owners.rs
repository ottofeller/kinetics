pub mod add;
pub mod delete;

use crate::commands::orgs::owners::add::AddOwnerCommand;
use crate::commands::orgs::owners::delete::DeleteOwnerCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum OwnersCommands {
    /// Add a new owner to an org
    Add(AddOwnerCommand),

    /// Demote an owner of an org
    Delete(DeleteOwnerCommand),
}
