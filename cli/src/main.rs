pub mod api;
mod cli;
mod commands;
mod config;
mod credentials;
mod envs;
mod error;
mod function;
mod logger;
mod migrations;
mod process;
mod project;
mod runner;
mod secrets;
mod sqldb;
pub mod tools;
use crate::cli::run as old_run;
use crate::commands::Commands;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use clap::Parser;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

/// Derive a runner from the command and run it
async fn run(command: impl Runnable) {
    let run = command.runner().run().await;

    if run.is_err() {
        println!("Error\n{}", run.unwrap_err())
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let cli = Cli::try_parse();

    // The command has not yet been transitioned to the new engine
    if cli.is_err() {
        return old_run(None).await;
    }

    // Match all commands here, in one place
    Ok(match cli.unwrap().command.unwrap() {
        Commands::Invoke(cmd) => run(cmd).await,
        Commands::Deploy(cmd) => run(cmd).await,
        Commands::Build(cmd) => run(cmd).await,
        Commands::Login(cmd) => run(cmd).await,

        Commands::Func(func) => match func {
            commands::func::FuncCommands::Stats(cmd) => run(cmd).await,
        },
    })
}
