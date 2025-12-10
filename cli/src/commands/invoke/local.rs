use super::docker::Docker;
use super::service::{LocalDynamoDB, LocalQueue, LocalSqlDB};
use crate::config::build_config;
use crate::function::Function;
use crate::process::Process;
use crate::project::Project;
use crate::secrets::Secrets;
use color_eyre::owo_colors::OwoColorize;
use eyre::WrapErr;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Invoke the function locally
#[allow(clippy::too_many_arguments)]
pub async fn invoke(
    function: &Function,
    project: &Project,
    payload: Option<&str>,
    headers: Option<&str>,
    url_path: Option<&str>,

    // DynamoDbB table to provision
    table: Option<&str>,

    is_sqldb_enabled: bool,
    is_queue_enabled: bool,
    is_migrations_enabled: bool,
    migrations_path: Option<&str>,
) -> eyre::Result<()> {
    let home = std::env::var("HOME").wrap_err("Can not read HOME env var")?;
    let mut secrets_envs = HashMap::new();

    // Envs with the prefix are then processed and provisioned as secrets
    for (name, value) in Secrets::load() {
        secrets_envs.insert(format!("KINETICS_SECRET_{}", name.clone()), value);
    }

    let invoke_dir = Path::new(&home).join(format!(".kinetics/{}", project.name));
    let display_path = format!("{}/src/bin/{}Local.rs", invoke_dir.display(), function.name);

    println!(
        "\n{} {} {}...",
        console::style("Invoking function").green().bold(),
        console::style("from").dimmed(),
        console::style(&display_path).underlined().bold()
    );

    let mut docker = Docker::new(&PathBuf::from(&build_config()?.kinetics_path));

    let mut local_environment = HashMap::from([
        ("KINETICS_IS_LOCAL", "true".to_string()),
        // Local SQS uses a fixed account id
        ("KINETICS_CLOUD_ACCOUNT_ID", "000000000000".to_string()),
    ]);

    if is_sqldb_enabled {
        let mut sqldb = LocalSqlDB::new(project);

        if is_migrations_enabled {
            sqldb.with_migrations(migrations_path);
        }

        local_environment.insert(
            "KINETICS_SQLDB_LOCAL_CONNECTION_STRING",
            sqldb.connection_string(),
        );
        docker.with_sqldb(sqldb);
    }

    if is_queue_enabled {
        let queue = LocalQueue::new();
        local_environment.insert("KINETICS_QUEUE_NAME", queue.name());
        local_environment.insert("KINETICS_QUEUE_ENDPOINT_URL", queue.endpoint_url());
        docker.with_queue(queue);
    }

    if let Some(table) = table {
        docker.with_dynamodb(LocalDynamoDB::new(table));
    }

    docker.start()?;
    docker.provision().await?;

    let mut aws_credentials = HashMap::new();

    // Do not mock AWS endpoint when not needed
    if table.is_some() || is_queue_enabled {
        aws_credentials.insert("AWS_IGNORE_CONFIGURED_ENDPOINT_URLS", "false");
        aws_credentials.insert("AWS_ENDPOINT_URL", "http://localhost:8000");
        aws_credentials.insert("AWS_ACCESS_KEY_ID", "key");
        aws_credentials.insert("AWS_SECRET_ACCESS_KEY", "secret");
    }

    // Start the command with piped stdout and stderr
    let child = Command::new("cargo")
        .args(["run", "--bin", &format!("{}Local", function.name)])
        .envs(secrets_envs)
        .envs(aws_credentials)
        .envs(local_environment)
        .envs(function.environment())
        .env("KINETICS_INVOKE_PAYLOAD", payload.unwrap_or("{}"))
        .env("KINETICS_INVOKE_HEADERS", headers.unwrap_or("{}"))
        .env("KINETICS_INVOKE_URL_PATH", url_path.unwrap_or_default())
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
