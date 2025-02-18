mod build;
mod client;
mod crat;
mod deploy;
mod function;
mod login;
mod parser;
mod secret;
use crate::build::{build, prepare_crates};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use crat::Crate;
use eyre::{Ok, WrapErr};
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

fn skypath() -> eyre::Result<PathBuf> {
    Ok(Path::new(&std::env::var("HOME").wrap_err("Can not read HOME env var")?).join(".sky"))
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Build,

    Deploy {
        #[arg(short, long)]
        is_directly: bool,
    },

    Login {
        #[arg()]
        email: String,
    },
}

/// Return crate info from Cargo.toml
fn crat() -> eyre::Result<Crate> {
    let path = std::env::current_dir().wrap_err("Failed to get current dir")?;
    Ok(Crate::new(path)?)
}

/// Return the list of dirs with functions to deploy
fn functions() -> eyre::Result<Vec<Function>> {
    let directories = prepare_crates(skypath()?, crat()?)?;

    let functions = directories
        .into_iter()
        .map(|p| Function::new(&p).unwrap())
        .collect();

    Ok(functions)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Build) => {
            let functions = functions().unwrap();

            if let Err(error) = build(&functions) {
                println!("{error}");
                return;
            }
        }
        Some(Commands::Deploy { is_directly }) => {
            let mut functions = functions().unwrap();

            if let Err(error) = build(&functions) {
                println!("{error:?}");
                return;
            }

            if let Err(error) = deploy::deploy(&mut functions, is_directly).await {
                println!("{error:?}");
                return;
            }
        }
        Some(Commands::Login { email }) => {
            login(email).await.unwrap();
        }
        None => {}
    }
}
