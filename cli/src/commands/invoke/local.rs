use super::docker::Docker;
use super::poller::LocalQueuePoller;
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
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

impl InvokeRunner<'_> {
    /// Invoke the function locally
    #[allow(clippy::too_many_arguments)]
    pub async fn local(
        &self,
        function: &Function,
        all_functions: &[Function],
        migrations_path: Option<&str>,
    ) -> eyre::Result<()> {
        let payload = self.resolve_payload(&function.role)?;

        let project = self.project(&self.command.project).await?;
        let mut secrets_envs = HashMap::new();

        // Envs with the prefix are then processed and provisioned as secrets.
        // Member secrets take priority over workspace root ones.
        let secrets = if project.workspace.root_path == project.path {
            Secrets::from_files(&[&project.path])
        } else {
            Secrets::from_files(&[&project.workspace.root_path, &project.path])
        }
        .unwrap_or_else(Secrets::from_env);
        for (name, value) in secrets {
            secrets_envs.insert(format!("KINETICS_SECRET_{}", name.clone()), value);
        }

        let invoke_dir = project.build_path()?;
        let display_path = format!(
            "{}/{}/src/bin/{}Local.rs",
            invoke_dir.display(),
            function.name,
            function.name
        );

        let mut docker = Docker::new(&PathBuf::from(&build_config()?.kinetics_path));

        let mut local_environment = HashMap::from([("KINETICS_IS_LOCAL", "true".to_string())]);

        if self.command.with_database {
            let mut sqldb = LocalSqlDB::new(&project, self.writer);

            if self.command.with_migrations.is_some() {
                sqldb.with_migrations(migrations_path);
            }

            local_environment.insert(
                "KINETICS_SQLDB_LOCAL_CONNECTION_STRING",
                sqldb.connection_string(),
            );
            docker.with_sqldb(sqldb);
        }

        // Resolve the workers to consume the queues with, if any were requested.
        // Duplicates are dropped while preserving the user order.
        let worker_names: Vec<&str> = match self.command.with_worker.as_deref() {
            None => Vec::new(),
            Some(list) => {
                let mut names: Vec<&str> = Vec::new();

                for name in list
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }

                names
            }
        };

        let workers = worker_names
            .into_iter()
            .map(|w| {
                Function::find_by_name(all_functions, w).map_err(|_| {
                    eyre::eyre!(
                "Worker {w} not found in the project. Use `kinetics func list` to see all names"
            )
                })
            })
            .collect::<eyre::Result<Vec<_>>>()?;

        if self.command.with_queue || self.command.with_worker.is_some() {
            // The generic queue impersonates any consumer without a dedicated
            // queue, so it backs both flags.
            let unnamed_queue = LocalQueue::new();
            local_environment.insert("KINETICS_QUEUE_NAME", unnamed_queue.name().to_string());
            local_environment.insert("KINETICS_QUEUE_ENDPOINT_URL", unnamed_queue.endpoint_url());
            // Local SQS uses a fixed account id
            local_environment.insert(
                "KINETICS_CLOUD_ACCOUNT_ID",
                unnamed_queue.account_id().to_string(),
            );
            docker.with_queue(unnamed_queue);

            // Provision a named queue per requested worker
            // and pass names to the invoked function.
            if !workers.is_empty() {
                let named_queues = workers
                    .iter()
                    .map(|worker| worker.name.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                local_environment.insert("KINETICS_LOCAL_QUEUE_NAMES", named_queues);

                for worker in &workers {
                    docker.with_queue(LocalQueue::named(&worker.name));
                }
            }
        }

        if let Some(table) = self.command.table.clone() {
            docker.with_dynamodb(LocalDynamoDB::new(&table));
        }

        docker.start(self.writer)?;
        docker.provision().await?;

        let mut aws_credentials = HashMap::new();

        // Do not mock AWS endpoint when not needed
        if self.command.table.is_some()
            || self.command.with_queue
            || self.command.with_worker.is_some()
        {
            aws_credentials.insert("AWS_IGNORE_CONFIGURED_ENDPOINT_URLS", "false");
            aws_credentials.insert("AWS_ENDPOINT_URL", "http://localhost:8000");
            aws_credentials.insert("AWS_ACCESS_KEY_ID", "key");
            aws_credentials.insert("AWS_SECRET_ACCESS_KEY", "secret");
            aws_credentials.insert("AWS_REGION", "us-east-1");
        }

        self.writer
            .text(&format!(
                "\n{} function {} {}...\n",
                console::style("Invoking").bold(),
                console::style("from").dimmed(),
                console::style(&display_path).underlined().bold()
            ))
            .map_err(|e| eyre::eyre!(e))?;

        // Invoke the main function.
        let output = self.invoke_local_binary(
            function,
            &invoke_dir,
            &secrets_envs,
            &aws_credentials,
            &local_environment,
            payload.as_deref(),
        )?;

        // Start polling and drain each named queue,
        // invoke the corresponding worker for every message.
        for worker in workers {
            let mut poller = LocalQueuePoller::new(LocalQueue::named(&worker.name));
            let messages = poller.drain_queue().await?;

            for body in messages {
                // Deliver each message as a batch of one.
                let payload = serde_json::json!([body]).to_string();

                self.writer
                    .text(&format!(
                        "\n{} worker {}...\n",
                        console::style("Invoking").bold(),
                        worker.name
                    ))
                    .map_err(|e| eyre::eyre!(e))?;

                self.invoke_local_binary(
                    &worker,
                    &invoke_dir,
                    &secrets_envs,
                    &aws_credentials,
                    &local_environment,
                    Some(&payload),
                )?;
            }

            // Stop the poller once its queue is drained.
            poller.abort();
        }

        self.writer
            .json(json!({ "success": true, "output": output }))?;

        Ok(())
    }

    /// Spawn and log the `*Local` binary for the given function.
    ///
    /// Returns the captured stdout of the invocation.
    fn invoke_local_binary(
        &self,
        function: &Function,
        invoke_dir: &PathBuf,
        secrets_envs: &HashMap<String, String>,
        aws_credentials: &HashMap<&str, &str>,
        local_environment: &HashMap<&str, String>,
        payload: Option<&str>,
    ) -> eyre::Result<String> {
        let mut command = Command::new("cargo");

        command
            .args(["run", "--bin", &format!("{}Local", function.name)])
            .envs(secrets_envs)
            .envs(aws_credentials)
            .envs(local_environment)
            .envs(function.environment())
            .env(
                "KINETICS_INVOKE_HEADERS",
                self.command.headers.clone().unwrap_or("{}".into()),
            )
            .env(
                "KINETICS_INVOKE_URL_PATH",
                self.command.url_path.clone().unwrap_or_default(),
            )
            .current_dir(invoke_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Pass the payload through a temporary file to avoid process environment size limits.
        // Keep the `TempPath` alive until the invocation completes so the file is not deleted
        // before the spawned binary has a chance to read it.
        let payload_file: Option<tempfile::TempPath> = if let Some(payload) = payload {
            let mut file = tempfile::NamedTempFile::new()
                .wrap_err("Failed to create temporary payload file")?;
            file.write_all(payload.as_bytes())
                .wrap_err("Failed to write temporary payload file")?;
            let temp_path = file.into_temp_path();
            let absolute_path = temp_path
                .canonicalize()
                .wrap_err("Failed to resolve temporary payload file path")?;
            command.arg("--").arg(absolute_path.as_os_str());
            Some(temp_path)
        } else {
            None
        };

        let child = command.spawn().wrap_err("Failed to execute cargo run")?;

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

        drop(payload_file);

        Ok(process.output())
    }
}
