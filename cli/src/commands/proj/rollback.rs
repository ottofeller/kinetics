use crate::api::stack;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct RollbackCommand {
    /// Specific version to rollback to (optional)
    #[arg(short, long)]
    version_id: Option<String>,
}

impl Runnable for RollbackCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        RollbackRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct RollbackRunner<'a> {
    command: RollbackCommand,
    writer: &'a Writer,
}

impl Runner for RollbackRunner<'_> {
    /// Rollback a project by one version or to a specific version
    ///
    /// Consequent rollbacks are possible and will revert one version at a time
    /// If version is specified, rollback to that specific version
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;
        let client = self.api_client().await?;

        let versions: stack::versions::Response = client
            .request(
                "/stack/versions",
                stack::versions::Request {
                    name: project.name.to_string(),
                },
            )
            .await
            .wrap_err("Failed to fetch versions")
            .map_err(|e| self.server_error(Some(e.into())))?;

        if versions.versions.len() < 2 && self.command.version_id.is_none() {
            self.writer.text(&format!(
                "{}\n",
                console::style("Nothing to rollback, there is only one version").yellow()
            ))?;

            self.writer.json(
                json!({"success": true, "message": "Nothing to rollback, no other versions"}),
            )?;

            return Ok(());
        }

        let target_version = match &self.command.version_id {
            Some(v) => {
                // Find the specified version in the list
                if let Some(target) = versions.versions.iter().find(|ver| &ver.version_id == v) {
                    target.clone()
                } else {
                    return Err(self.error(
                        Some(&format!("Version {} not found", v)),
                        Some("Use 'kinetics proj versions' to see available versions."),
                        None,
                    ));
                }
            }
            None => {
                // Default behavior: rollback to previous version
                versions.versions[1].clone()
            }
        };

        self.writer.text(&format!(
            "{} {} {}...\n\n",
            console::style("Rolling back").bold().green(),
            console::style("to").dim(),
            console::style(format!(
                "v{} ({})",
                target_version.version_id,
                target_version.updated_at.with_timezone(&chrono::Local)
            ))
            .bold(),
        ))?;

        client
            .post("/stack/rollback")
            .json(&stack::rollback::Request {
                name: project.name.to_string(),
                version_id: self.command.version_id.clone(),
            })
            .send()
            .await
            .wrap_err("Failed to rollback")
            .map_err(|e| self.server_error(Some(e.into())))?;

        let mut status = project
            .status()
            .await
            .map_err(|e| self.server_error(Some(e.into())))?;

        // Poll the status of the rollback
        while status.status == "IN_PROGRESS" {
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

            status = project
                .status()
                .await
                .map_err(|e| self.server_error(Some(e.into())))?;
        }

        if status.status == "FAILED" {
            return Err(self.error(
                Some("Rollback failed"),
                Some("Try again in a few minutes."),
                None,
            ));
        }

        self.writer
            .text(&format!("{}\n", console::style("Done").green()))?;

        self.writer.json(json!({"success": true}))?;
        Ok(())
    }
}
