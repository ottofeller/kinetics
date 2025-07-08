use crate::config::build_config;
use crate::crat::Crate;
use crate::function::Function;
use crate::process::Process;
use crate::secret::Secret;
use color_eyre::owo_colors::OwoColorize;
use super::dynamodb::LocalDynamoDB;
use eyre::WrapErr;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Invoke the function locally
pub async fn invoke(
    function: &Function,
    crat: &Crate,
    payload: &str,
    headers: &str,

    // DynamoDbB table to provision
    table: Option<&str>,
) -> eyre::Result<()> {
    let home = std::env::var("HOME").wrap_err("Can not read HOME env var")?;

    // Load secrets from .env.secrets if it exists
    let mut secrets = HashMap::new();

    for secret in Secret::from_dotenv() {
        secrets.insert(
            format!("KINETICS_SECRET_{}", secret.name.clone()),
            secret.value(),
        );
    }

    let invoke_dir = Path::new(&home).join(format!(".kinetics/{}", crat.name));
    let display_path = format!("{}/src/bin/{}Local.rs", invoke_dir.display(), function.name);

    println!(
        "\n{} {} {}...",
        console::style("Invoking function").green().bold(),
        console::style("from").dimmed(),
        console::style(&display_path).underlined().bold()
    );

    let mut dynamodb = LocalDynamoDB::new(&PathBuf::from(&build_config()?.build_path));

    if table.is_some() {
        dynamodb.start()?;
        dynamodb.provision(table.unwrap()).await?;
    }

    let mut aws_credentials = HashMap::new();

    // Do not mock AWS endpoint when not needed
    if table.is_some() {
        aws_credentials.insert("AWS_IGNORE_CONFIGURED_ENDPOINT_URLS", "false");
        aws_credentials.insert("AWS_ENDPOINT_URL", "http://localhost:8000");
        aws_credentials.insert("AWS_ACCESS_KEY_ID", "key");
        aws_credentials.insert("AWS_SECRET_ACCESS_KEY", "secret");
    }

    // Start the command with piped stdout and stderr
    let child = Command::new("cargo")
        .args(["run", "--bin", &format!("{}Local", function.name)])
        .envs(secrets)
        .envs(aws_credentials)
        .envs(function.environment()?)
        .env("KINETICS_INVOKE_PAYLOAD", payload)
        .env("KINETICS_INVOKE_HEADERS", headers)
        .current_dir(&invoke_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .wrap_err("Failed to execute cargo run")?;

    let mut process = Process::new(child);
    let status = process.log()?;

    if !status.success() {
        process.print_error();
        return Err(eyre::eyre!("Failed with exit code: {}", status));
    }

    // If successful, print the full stdout
    process.print();

    Ok(())
}
