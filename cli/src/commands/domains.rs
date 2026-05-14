pub mod add;

use clap::Subcommand;

use crate::commands::domains::add::AddCommand;

#[derive(Subcommand)]
pub enum DomainsCommands {
    Add(AddCommand),
}
