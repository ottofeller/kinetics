pub mod destroy;
pub mod list;
pub mod rollback;
pub mod versions;
use crate::commands::proj::destroy::DestroyCommand;
use crate::commands::proj::list::ListCommand;
use crate::commands::proj::rollback::RollbackCommand;
use crate::commands::proj::versions::VersionsCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum ProjCommands {
    /// [DANGER] Destroy a project
    Destroy(DestroyCommand),

    /// Rollback to older version
    Rollback(RollbackCommand),

    /// List projects
    List(ListCommand),

    /// List all available versions
    Versions(VersionsCommand),
}
