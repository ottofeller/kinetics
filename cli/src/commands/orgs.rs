pub mod create;
pub mod delete;
pub mod list;
pub mod members;

use crate::commands::orgs::create::CreateCommand;
use crate::commands::orgs::delete::DeleteCommand;
use crate::commands::orgs::list::ListCommand;
use crate::commands::orgs::members::MembersCommands;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum OrgsCommands {
    /// Create new org with you as an owner
    Create(CreateCommand),

    /// Delete an org
    Delete(DeleteCommand),

    /// List all orgs you belong to
    List(ListCommand),

    /// Manage org members
    #[clap(subcommand)]
    Members(MembersCommands),
}
