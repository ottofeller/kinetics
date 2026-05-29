pub mod delete;
pub mod invite;

use crate::commands::orgs::members::delete::DeleteMemberCommand;
use crate::commands::orgs::members::invite::InviteMemberCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum MembersCommands {
    /// Invite new member to an org
    Invite(InviteMemberCommand),

    /// Remove a member from an org
    Delete(DeleteMemberCommand),
}
