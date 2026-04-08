pub mod create;
pub mod list;
use crate::commands::orgs::create::CreateCommand;
use crate::commands::orgs::list::ListCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum OrgsCommands {
    /// Create new org with you as an owner
    Create(CreateCommand),

    /// List all orgs you belong to
    List(ListCommand),
}
