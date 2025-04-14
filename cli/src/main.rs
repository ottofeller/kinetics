mod build;
mod client;
mod config;
mod crat;
mod destroy;
mod function;
mod invoke;
mod login;
mod secret;

use crate::build::pipeline::Pipeline;
use crate::build::prepare_crates;
use crate::config::config;
use crate::destroy::destroy;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use crat::Crate;
use eyre::WrapErr;
use function::Function;
use invoke::invoke;
use login::login;
use std::path::{Path, PathBuf};

/// Credentials to be used with API
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    email: String,
    token: String,
    expires_at: DateTime<Utc>,
}

fn api_url(path: &str) -> String {
    format!("{}{}", config().api_base, path)
}

fn build_path() -> eyre::Result<PathBuf> {
    Ok(Path::new(&std::env::var("HOME").wrap_err("Can not read HOME env var")?).join(".kinetics"))
}

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
    /// Build your serverless functions
    Build {
        /// Maximum number of parallel concurrent builds
        #[arg(short, long, default_value_t = 6)]
        max_concurrency: usize,
    },

    /// Deploy your serverless functions to the cloud
    Deploy {
        /// Upload directly to S3 and bypass the backend service
        #[arg(short, long)]
        is_directly: bool,

        /// Maximum number of parallel concurrent builds
        #[arg(short, long, default_value_t = 6)]
        max_concurrency: usize,
    },

    /// Destroy your serverless functions
    Destroy {},

    /// Login to Kinetics platform
    Login {
        /// Your registered email address
        #[arg()]
        email: String,
    },

    /// Invoke a functions
    Invoke {
        #[arg()]
        name: String,
    },
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let cli = Cli::parse();
    let crat = Crate::from_current_dir()?;
    let directories = prepare_crates(build_path()?, crat.clone())?;

    // Functions to deploy
    let functions = directories
        .into_iter()
        .map(|p| Function::new(&p).unwrap())
        .collect();

    color_eyre::config::HookBuilder::default()
        .display_location_section(false)
        .display_env_section(false)
        .theme(color_eyre::config::Theme::new())
        .install()?;

    match &cli.command {
        Some(Commands::Build { max_concurrency }) => {
            Pipeline::builder()
                .set_max_concurrent(*max_concurrency)
                .with_deploy_enabled(false)
                .set_crat(Crate::from_current_dir()?)
                .build()
                .wrap_err("Failed to build pipeline")?
                .run(functions)
                .await?;

            Ok(())
        }
        Some(Commands::Deploy {
            is_directly,
            max_concurrency,
        }) => {
            Pipeline::builder()
                .set_max_concurrent(*max_concurrency)
                .with_deploy_enabled(true)
                .set_crat(Crate::from_current_dir()?)
                .with_directly(*is_directly)
                .build()
                .wrap_err("Failed to build pipeline")?
                .run(functions)
                .await?;

            Ok(())
        }
        Some(Commands::Login { email }) => {
            let is_new_session = login(email).await.wrap_err("Login failed")?;

            println!(
                "{} {} {}",
                console::style(if is_new_session {
                    "Successfully logged in"
                } else {
                    "Already logged in"
                })
                .green()
                .bold(),
                console::style("via").dim(),
                console::style(email).underlined().bold()
            );

            Ok(())
        }
        Some(Commands::Destroy {}) => {
            destroy(&Crate::from_current_dir()?)
                .await
                .wrap_err("Failed to destroy the project")?;

            Ok(())
        }
        Some(Commands::Invoke { name }) => {
            invoke(
                functions
                    .iter()
                    .find(|f| name.eq(&f.name().wrap_err("Function's meta is invalid").unwrap()))
                    .unwrap(),

                &crat,
            )
            .wrap_err("Failed to invoke the function")?;
            Ok(())
        }
        None => Ok(()),
    }
}
