pub mod create;
pub mod delete;
pub mod invite;
pub mod list;
use crate::commands::orgs::create::CreateCommand;
use crate::commands::orgs::delete::DeleteCommand;
use crate::commands::orgs::invite::InviteCommand;
use crate::commands::orgs::list::ListCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum OrgsCommands {
    /// Create new org with you as an owner
    Create(CreateCommand),

    /// Delete an org
    Delete(DeleteCommand),

    /// Invite a member to an org (sends an email)
    Invite(InviteCommand),

    /// List all orgs you belong to
    List(ListCommand),
}
