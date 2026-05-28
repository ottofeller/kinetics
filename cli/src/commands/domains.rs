pub mod add;

use crate::commands::domains::add::AddCommand;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum DomainsCommands {
    Add(AddCommand),
}
