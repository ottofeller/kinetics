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
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Build {
        #[arg(short, long, default_value_t = 4)]
        max_concurrent: usize,
    },

    Deploy {
        #[arg(short, long)]
        is_directly: bool,

        #[arg(short, long, default_value_t = 4)]
        max_concurrent: usize,
    },

    Login {
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

    match &cli.command {
        Some(Commands::Build { max_concurrent }) => {
            Pipeline::builder()
                .set_max_concurrent(*max_concurrent)
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
            max_concurrent,
        }) => {
            Pipeline::builder()
                .set_max_concurrent(*max_concurrent)
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
                println!("Login successful");
                Ok(())
            }
            Err(error) => Err(error),
        },
        None => Ok(()),
    }
}
