mod client;
mod crat;
mod deploy;
mod function;
mod login;
mod secret;
use clap::{Parser, Subcommand};
use crat::Crate;
use eyre::{Ok, WrapErr};
use function::Function;
use login::login;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
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
    Deploy,

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
    let mut result = vec![];
    let crat = crat()?;

    for entry in std::fs::read_dir(
        &skypath()
            .wrap_err("Failed to resolve sky path")?
            .join(crat.name),
    )
    .wrap_err("Failed to read dir")?
    {
        let path = entry.wrap_err("Failed to get dir entry")?.path();

        if path.is_dir() {
            result.push(Function::new(&path)?);
        }
    }

    Ok(result)
}

/// Build all assets and CFN templates
fn build() -> eyre::Result<()> {
    let threads: Vec<_> = functions()?
        .into_iter()
        .map(|function| std::thread::spawn(move || function.build()))
        .collect();

    for thread in threads {
        thread.join().unwrap()?;
    }

    println!("Done!");
    Ok(())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Build) => {
            if let Err(error) = build() {
                println!("{error}");
                return;
            }
        }
        Some(Commands::Deploy) => {
            if let Err(error) = build() {
                println!("{error:?}");
                return;
            }

            if let Err(error) = deploy::deploy().await {
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
