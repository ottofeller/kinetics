pub mod api;
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

    // Match all commands here, in one place
    Ok(match cli.unwrap().command.unwrap() {
        Commands::Auth(auth) => match auth {
            commands::auth::AuthCommands::Logout(cmd) => run(cmd).await,
            commands::auth::AuthCommands::Tokens(cmd) => match cmd {
                commands::auth::tokens::TokensCommands::Create(cmd) => run(cmd).await,
                commands::auth::tokens::TokensCommands::Delete(cmd) => run(cmd).await,
                commands::auth::tokens::TokensCommands::List(cmd) => run(cmd).await,
            },
        },

        Commands::Cicd(cicd) => match cicd {
            commands::cicd::CicdCommands::Init(cmd) => run(cmd).await,
        },

        Commands::Envs(envs) => match envs {
            commands::envs::EnvsCommands::List(cmd) => run(cmd).await,
        },

        Commands::Func(func) => match func {
            commands::func::FuncCommands::List(cmd) => run(cmd).await,
            commands::func::FuncCommands::Stats(cmd) => run(cmd).await,
            commands::func::FuncCommands::Logs(cmd) => run(cmd).await,
            commands::func::FuncCommands::Stop(cmd) => run(cmd).await,
            commands::func::FuncCommands::Start(cmd) => run(cmd).await,
        },

        Commands::Migrations(migrations) => match migrations {
            commands::migrations::MigrationsCommands::Create(cmd) => run(cmd).await,
            commands::migrations::MigrationsCommands::Apply(cmd) => run(cmd).await,
        },

        Commands::Proj(proj) => match proj {
            commands::proj::ProjCommands::Destroy(cmd) => run(cmd).await,
            commands::proj::ProjCommands::Rollback(cmd) => run(cmd).await,
            commands::proj::ProjCommands::List(cmd) => run(cmd).await,
            commands::proj::ProjCommands::Versions(cmd) => run(cmd).await,
        },

        Commands::Init(cmd) => run(cmd).await,
        Commands::Invoke(cmd) => run(cmd).await,
        Commands::Deploy(cmd) => run(cmd).await,
        Commands::Build(cmd) => run(cmd).await,
        Commands::Login(cmd) => run(cmd).await,
    })
}
