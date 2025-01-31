mod crat;
mod function;
mod secret;
use backend::deploy::{self, BodyCrate};
use clap::{Parser, Subcommand};
use crat::Crate;
use eyre::{Ok, WrapErr};
use function::Function;
use secret::Secret;
use std::path::{Path, PathBuf};
static API_BASE: &str = "https://backend.usekinetics.com";

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

/// Bundle assets and upload to S3, assuming all functions are built
fn bundle(functions: &Vec<Function>) -> eyre::Result<()> {
    for function in functions {
        function.bundle()?;
    }

    Ok(())
}

/// All bundled assets to S3
async fn upload(functions: &Vec<Function>) -> eyre::Result<()> {
    for function in functions {
        #[derive(serde::Deserialize)]
        struct PresignedUrl {
            url: String,
        }

        let path = function.bundle_path();
        let key = path.file_name().unwrap().to_str().unwrap();
        println!("Uploading {path:?}...");
        let client = reqwest::Client::new();

        let presigned = client
            .post(api_url("/upload"))
            .json(&serde_json::json!({ "key": key }))
            .send()
            .await?
            .json::<PresignedUrl>()
            .await?;

        client
            .put(&presigned.url)
            .body(tokio::fs::read(&path).await?)
            .send()
            .await?
            .error_for_status()?;
    }

    Ok(())
}

/// Build and deploy all assets using CFN template
async fn deploy() -> eyre::Result<()> {
    let crat = crat().unwrap();
    let functions = functions().wrap_err("Failed to bundle assets")?;
    let client = reqwest::Client::new();
    println!("Deploying \"{}\"...", crat.name);
    bundle(&functions)?;
    upload(&functions).await?;

    client
        .post(api_url("/deploy"))
        .json(&serde_json::json!(deploy::JsonBody {
            crat: BodyCrate {
                toml: crat.toml_string.clone(),
            },
            functions: functions
                .iter()
                .map(|f| {
                    deploy::BodyFunction {
                        name: f.name().unwrap().to_string(),
                        s3key: f.bundle_name(),
                        toml: f.toml_string().unwrap(),
                    }
                })
                .collect(),
            secrets: vec![],
        }))
        .send()
        .await
        .wrap_err("Deployment request failed")?;

    let secrets = Secret::from_dotenv(&crat.name)?;

    for secret in secrets.iter() {
        secret.sync().await?;
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

            if let Err(error) = deploy().await {
                println!("{error:?}");
                return;
            }
        }
        None => {}
    }
}
