use crate::commands::build::pipeline::Pipeline;
use crate::config::deploy::DeployConfig;
use crate::error::Error;
use crate::logger::Logger;
use crate::project::Project;
use clap::{ArgAction, Parser, Subcommand};
use eyre::{Ok, WrapErr};
use std::sync::Arc;

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
}

#[derive(Subcommand)]
enum Commands {
    /// Legacy/deprecated deploy path. Use `kinetics deploy` instead.
    DeployOld {
        /// Maximum number of parallel concurrent builds
        #[arg(short, long, default_value_t = 3)]
        max_concurrency: usize,

        /// Deploy only environment variables instead of full deployment
        #[arg(short, long, action = ArgAction::SetTrue)]
        envs: bool,

        /// Use hotswap deployment for faster updates
        #[arg(long, action = ArgAction::SetTrue)]
        hotswap: bool,

        #[arg(value_delimiter = ',')]
        functions: Vec<String>,
    },
}

pub async fn run(deploy_config: Option<Arc<dyn DeployConfig>>) -> Result<(), Error> {
    Logger::init();
    let cli = Cli::parse();

    // DEPRECATED This is left to maintain compatibility with the backend
    // Global commands
    match &cli.command {
        Some(Commands::DeployOld {
            functions,
            max_concurrency,
            hotswap,
            ..
        }) => {
            Pipeline::builder()
                .set_max_concurrent(*max_concurrency)
                .with_deploy_enabled(true)
                .with_hotswap(*hotswap)
                .with_deploy_config(deploy_config)
                .set_project(Project::from_current_dir()?)
                .build()
                .wrap_err("Failed to build pipeline")?
                .run(functions)
                .await
        }
        _ => Ok(()),
    }
    .map_err(Error::from)
}
