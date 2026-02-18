pub mod list;
pub mod logs;
pub mod stats;
pub mod toggle;

use crate::commands::func::list::ListCommand;
use crate::commands::func::logs::LogsCommand;
use crate::commands::func::stats::StatsCommand;
use crate::commands::func::toggle::{StartCommand, StopCommand};
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum FuncCommands {
    /// List all functions in the project
    List(ListCommand),

    /// Get function stats
    ///
    /// Includes run statistics (error/success/total count).
    Stats(StatsCommand),

    /// Show function logs
    Logs(LogsCommand),

    /// Stop function in the cloud
    Stop(StopCommand),

    /// Start previously stopped function
    Start(StartCommand),
}
