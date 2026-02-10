pub(crate) mod logout;
pub(crate) mod tokens;

use crate::commands::auth::logout::LogoutCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum AuthCommands {
    #[clap(subcommand)]
    Tokens(tokens::TokensCommands),

    /// Log out from server
    Logout(LogoutCommand),
}
