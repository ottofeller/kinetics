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
mod writer;
use crate::commands::Commands;
use crate::error::Error;
use crate::logger::Logger;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use clap::{ArgAction, Parser};
use serde_json::json;

#[derive(Parser)]
#[command(
    arg_required_else_help = true,
    name = "kinetics",
    version,
    about = "CLI tool for building and deploying serverless Rust functions",
    long_about = "A comprehensive CLI for managing Kinetics serverless Rust functions, including building, deploying and managing your infrastructure."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Return all output in valid json
    #[arg(short, long, action = ArgAction::SetTrue)]
    json: bool,

    /// Writer for all outputs
    #[arg(skip)]
    writer: Writer,
}

impl Cli {
    /// Derive a runner from the command and run it
    pub(crate) async fn run(&self, command: &impl Runnable) {
        let run = command.runner(&self.writer).run().await;

        if run.is_err() {
            let error = run.unwrap_err();
            log::error!("{:?}", error);

            self.writer
                .error(&format!(
                    "\n\n{}\n{error}\n",
                    console::style("Error").red().bold()
                ))
                .unwrap();

            self.writer
                .json(json!({"success": false, "error": error.to_string()}))
                .unwrap();

            std::process::exit(1)
        }
    }

    pub(crate) fn set_writer(&mut self, writer: Writer) -> () {
        self.writer = writer;
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    Logger::init();
    let mut cli = Cli::parse();
    let writer = Writer::new(cli.json);
    cli.set_writer(writer);

    // Match all commands here, in one place
    Ok(match cli.command.as_ref().unwrap() {
        Commands::Auth(auth) => match auth {
            commands::auth::AuthCommands::Logout(cmd) => cli.run(cmd).await,
            commands::auth::AuthCommands::Tokens(cmd) => match cmd {
                commands::auth::tokens::TokensCommands::Create(cmd) => cli.run(cmd).await,
                commands::auth::tokens::TokensCommands::Delete(cmd) => cli.run(cmd).await,
                commands::auth::tokens::TokensCommands::List(cmd) => cli.run(cmd).await,
            },
        },

        Commands::Cicd(cicd) => match cicd {
            commands::cicd::CicdCommands::Init(cmd) => cli.run(cmd).await,
        },

        Commands::Envs(envs) => match envs {
            commands::envs::EnvsCommands::List(cmd) => cli.run(cmd).await,
        },

        Commands::Func(func) => match func {
            commands::func::FuncCommands::List(cmd) => cli.run(cmd).await,
            commands::func::FuncCommands::Stats(cmd) => cli.run(cmd).await,
            commands::func::FuncCommands::Logs(cmd) => cli.run(cmd).await,
            commands::func::FuncCommands::Stop(cmd) => cli.run(cmd).await,
            commands::func::FuncCommands::Start(cmd) => cli.run(cmd).await,
        },

        Commands::Migrations(migrations) => match migrations {
            commands::migrations::MigrationsCommands::Create(cmd) => cli.run(cmd).await,
            commands::migrations::MigrationsCommands::Apply(cmd) => cli.run(cmd).await,
        },

        Commands::Proj(proj) => match proj {
            commands::proj::ProjCommands::Destroy(cmd) => cli.run(cmd).await,
            commands::proj::ProjCommands::Rollback(cmd) => cli.run(cmd).await,
            commands::proj::ProjCommands::List(cmd) => cli.run(cmd).await,
            commands::proj::ProjCommands::Versions(cmd) => cli.run(cmd).await,
        },

        Commands::Init(cmd) => cli.run(cmd).await,
        Commands::Invoke(cmd) => cli.run(cmd).await,
        Commands::Deploy(cmd) => cli.run(cmd).await,
        Commands::Build(cmd) => cli.run(cmd).await,
        Commands::Login(cmd) => cli.run(cmd).await,
    })
}
