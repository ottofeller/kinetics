mod create;
mod delete;
mod list;
use clap::Subcommand;
use create::CreateCommand;
use delete::DeleteCommand;

use crate::commands::auth::tokens::list::ListCommand;

#[derive(Subcommand)]
pub(crate) enum TokensCommands {
    /// Create a new access token
    Create(CreateCommand),

    /// Delete an access token
    Delete(DeleteCommand),

    /// List all access tokens
    List(ListCommand),
}
