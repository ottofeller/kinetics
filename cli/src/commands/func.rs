pub mod list;
pub mod logs;
pub mod stats;
pub mod toggle;
use crate::commands::func::stats::StatsCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum FuncCommands {
    /// Get function statistics,
    /// that include run statistics (error/success/total count)
    /// as well as last call time and status.
    Stats(StatsCommand),
}
