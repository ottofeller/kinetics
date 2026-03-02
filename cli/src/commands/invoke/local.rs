use super::docker::Docker;
use super::service::{LocalDynamoDB, LocalQueue, LocalSqlDB};
use crate::commands::invoke::InvokeRunner;
use crate::config::build_config;
use crate::function::Function;
use crate::process::Process;
use crate::runner::Runner;
use crate::secrets::Secrets;
use color_eyre::owo_colors::OwoColorize;
use eyre::WrapErr;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

impl InvokeRunner<'_> {
    /// Invoke the function locally
    #[allow(clippy::too_many_arguments)]
    pub async fn local(
        &self,
        function: &Function,
        migrations_path: Option<&str>,
    ) -> eyre::Result<()> {
        let project = self.project().await?;
        let home = std::env::var("HOME").wrap_err("Can not read HOME env var")?;
        let mut secrets_envs = HashMap::new();

        // Envs with the prefix are then processed and provisioned as secrets
        for (name, value) in Secrets::load() {
            secrets_envs.insert(format!("KINETICS_SECRET_{}", name.clone()), value);
        }

        let invoke_dir = Path::new(&home).join(format!(".kinetics/{}", project.name));
        let display_path = format!("{}/src/bin/{}Local.rs", invoke_dir.display(), function.name);

        self.writer
            .text(&format!(
                "\n{} {} {}...\n",
                console::style("Invoking function").green().bold(),
                console::style("from").dimmed(),
                console::style(&display_path).underlined().bold()
            ))
            .map_err(|e| eyre::eyre!(e))?;

        let mut docker = Docker::new(&PathBuf::from(&build_config()?.kinetics_path));

        let mut local_environment = HashMap::from([
            ("KINETICS_IS_LOCAL", "true".to_string()),
            // Local SQS uses a fixed account id
            ("KINETICS_CLOUD_ACCOUNT_ID", "000000000000".to_string()),
        ]);

        if self.command.with_database {
            let mut sqldb = LocalSqlDB::new(&project);

            if self.command.with_migrations.is_some() {
                sqldb.with_migrations(migrations_path);
            }

            local_environment.insert(
                "KINETICS_SQLDB_LOCAL_CONNECTION_STRING",
                sqldb.connection_string(),
            );
            docker.with_sqldb(sqldb);
        }

        if self.command.with_queue {
            let queue = LocalQueue::new();
            local_environment.insert("KINETICS_QUEUE_NAME", queue.name());
            local_environment.insert("KINETICS_QUEUE_ENDPOINT_URL", queue.endpoint_url());
            docker.with_queue(queue);
        }

        if let Some(table) = self.command.table.clone() {
            docker.with_dynamodb(LocalDynamoDB::new(&table));
        }

        docker.start(self.writer)?;
        docker.provision().await?;

        let mut aws_credentials = HashMap::new();

        // Do not mock AWS endpoint when not needed
        if self.command.table.is_some() || self.command.with_queue {
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
            .env(
                "KINETICS_INVOKE_PAYLOAD",
                self.command.payload.clone().unwrap_or("{}".into()),
            )
            .env(
                "KINETICS_INVOKE_HEADERS",
                self.command.headers.clone().unwrap_or("{}".into()),
            )
            .env(
                "KINETICS_INVOKE_URL_PATH",
                self.command.url_path.clone().unwrap_or_default(),
            )
            .current_dir(&invoke_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .wrap_err("Failed to execute cargo run")?;

        let mut process = Process::new(child, self.writer);
        let status = process.log()?;

        if !status.success() {
            process.print_error()?;

            return Err(eyre::eyre!(
                "Invocation failed with status {}: {}",
                status,
                process.errors_output()
            ));
        }

        // If successful, print the full stdout
        process.print()?;

        self.writer
            .json(json!({"success": true, "output": process.output()}))?;

        Ok(())
    }
}
