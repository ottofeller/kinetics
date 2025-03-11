mod build;
mod client;
mod crat;
mod deploy;
mod function;
mod login;
mod parser;
mod pipeline;
mod secret;

use crate::build::prepare_crates;
use crate::pipeline::Pipeline;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use crat::Crate;
use eyre::WrapErr;
use function::Function;
use login::login;
use std::path::{Path, PathBuf};

static API_BASE: &str = "https://backend.usekinetics.com";

/// Credentials to be used with API
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    email: String,
    token: String,
    expires_at: DateTime<Utc>,
}

fn api_url(path: &str) -> String {
    format!("{}{}", API_BASE, path)
}

fn build_path() -> eyre::Result<PathBuf> {
    Ok(Path::new(&std::env::var("HOME").wrap_err("Can not read HOME env var")?).join(".kinetics"))
}

#[derive(Parser)]
#[command(
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

    /// Login to Kinetics platform
    Login {
        /// Your registered email address
        #[arg()]
        email: String,
    },
}

/// Return crate info from Cargo.toml
fn crat() -> eyre::Result<Crate> {
    let path = std::env::current_dir().wrap_err("Failed to get current dir")?;
    Crate::new(path)
}

/// Return the list of dirs with functions to deploy
fn functions() -> eyre::Result<Vec<Function>> {
    let directories = prepare_crates(build_path()?, crat()?)?;

    let functions = directories
        .into_iter()
        .map(|p| Function::new(&p).unwrap())
        .collect();

    Ok(functions)
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let cli = Cli::parse();

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
                .set_crat(crat()?)
                .build()
                .wrap_err("Failed to build pipeline")?
                .run(functions()?)
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
                .set_crat(crat()?)
                .with_directly(*is_directly)
                .build()
                .wrap_err("Failed to build pipeline")?
                .run(functions()?)
                .await?;

            Ok(())
        }
        Some(Commands::Login { email }) => match login(email).await {
            Ok(_) => {
                println!(
                    "{} {} {}",
                    console::style("You have been successfully logged in")
                        .green()
                        .bold(),
                    console::style("via").dim(),
                    console::style(email).underlined().bold()
                );

                Ok(())
            }
            Err(error) => {
                println!(
                    "{} {} {}",
                    console::style("Failed to login").red().bold(),
                    console::style("via").dim(),
                    console::style(email).underlined().bold()
                );

                Err(error)
            }
        },
        None => Ok(()),
    }
}
