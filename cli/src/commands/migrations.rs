mod apply;
mod create;
use crate::commands::migrations::apply::ApplyCommand;
use crate::commands::migrations::create::CreateCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum MigrationsCommands {
    /// Create a new migration file
    Create(CreateCommand),

    /// Apply migrations to remote DB
    Apply(ApplyCommand),
}
