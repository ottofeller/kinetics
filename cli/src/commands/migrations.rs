mod apply;
mod create;
mod preview;
use crate::commands::migrations::apply::ApplyCommand;
use crate::commands::migrations::create::CreateCommand;
use crate::commands::migrations::preview::PreviewCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum MigrationsCommands {
    /// Create a new migration file
    Create(CreateCommand),

    /// Apply migrations to remote DB
    Apply(ApplyCommand),

    /// List migrations that are not yet applied to the remote DB
    Preview(PreviewCommand),
}
