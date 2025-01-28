mod crat;
mod function;
mod secret;
mod template;
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use clap::{Parser, Subcommand};
use crat::Crate;
use eyre::{Ok, WrapErr};
use function::Function;
use secret::Secret;
use std::path::{Path, PathBuf};
use template::Template;
static BUCKET: &str = "kinetics-rust-builds";
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

#[derive(Clone, Debug)]
struct Queue {
    name: String,
    concurrency: u32,
}

#[derive(Clone, Debug)]
struct KvDb {
    name: String,
}

#[derive(Clone, Debug)]
enum Resource {
    Queue(Queue),
    KvDb(KvDb),
}

/// Build all assets and CFN templates
fn build() -> eyre::Result<()> {
    let project = crat()?;
    println!("Building \"{}\"...", project.name);

    for function in functions()? {
        function.build()?;
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
        let bucket_name = "my-lambda-function-code-test";
        let path = function.bundle_path();
        let key = path.file_name().unwrap().to_str().unwrap();
        let body = function.zip_stream().await?;

        let config = aws_config::defaults(BehaviorVersion::v2024_03_28())
            .load()
            .await;

        let client = Client::new(&config);

        client
            .put_object()
            .bucket(bucket_name)
            .key(key)
            .body(body)
            .send()
            .await
            .wrap_err("Failed to upload file to S3")?;
    }

    Ok(())
}

/// Check if the stack already exists
async fn is_exists(client: &aws_sdk_cloudformation::Client, name: &str) -> eyre::Result<bool> {
    let result = client
        .describe_stacks()
        .set_stack_name(Some(name.into()))
        .send()
        .await;

    if let Err(e) = &result {
        if let aws_sdk_cloudformation::error::SdkError::ServiceError(err) = e {
            if err.err().meta().code().unwrap().eq("ValidationError") {
                return Ok(false);
            } else {
                return Err(eyre::eyre!(
                    "Service error while describing stack: {:?}",
                    err
                ));
            }
        } else {
            return Err(eyre::eyre!("Failed to describe stack: {:?}", e));
        }
    }

    Ok(true)
}

/// Provision cloud resources using CFN template
async fn provision(template: &str) -> eyre::Result<()> {
    let config = aws_config::defaults(BehaviorVersion::v2024_03_28())
        .load()
        .await;

    let client = aws_sdk_cloudformation::Client::new(&config);
    let name = "sky-example";
    let capabilities = aws_sdk_cloudformation::types::Capability::CapabilityIam;

    if is_exists(&client, name).await? {
        client
            .update_stack()
            .capabilities(capabilities)
            .stack_name(name)
            .template_body(template)
            .send()
            .await
            .wrap_err("Failed to update stack")?;
    } else {
        client
            .create_stack()
            .capabilities(capabilities)
            .stack_name(name)
            .template_body(template)
            .send()
            .await
            .wrap_err("Failed to create stack")?;
    }

    Ok(())
}

/// Build and deploy all assets using CFN template
async fn deploy() -> eyre::Result<()> {
    let crat = crat().unwrap();
    let functions = functions().wrap_err("Failed to bundle assets")?;
    println!("Deploying \"{}\"...", crat.name);
    bundle(&functions)?;
    upload(&functions).await?;
    let secrets = Secret::from_dotenv(&crat.name)?;

    for secret in secrets.iter() {
        secret.sync().await?;
    }

    let template = Template::new(&crat, functions, secrets, BUCKET)?;
    println!("Provisioning resources:\n{}", template.template);
    provision(&template.template, &crat).await?;
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
