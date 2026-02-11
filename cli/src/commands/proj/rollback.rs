use crate::api::{client::Client, stack};
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use eyre::Context;

#[derive(clap::Args, Clone)]
pub(crate) struct RollbackCommand {
    /// Specific version to rollback to (optional)
    #[arg(short, long)]
    version: Option<u32>,
}

impl Runnable for RollbackCommand {
    fn runner(&self) -> impl Runner {
        RollbackRunner {
            command: self.clone(),
        }
    }
}

struct RollbackRunner {
    command: RollbackCommand,
}

impl Runner for RollbackRunner {
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
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        if versions.versions.len() < 2 && self.command.version.is_none() {
            println!(
                "{}",
                console::style("Nothing to rollback, there is only one version").yellow()
            );
            return Ok(());
        }

        let target_version = match self.command.version {
            Some(v) => {
                // Find the specified version in the list
                if let Some(target) = versions.versions.iter().find(|ver| ver.version == v) {
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

        println!(
            "{} {} {}...",
            console::style("Rolling back").bold().green(),
            console::style("to").dim(),
            console::style(format!(
                "v{} ({})",
                target_version.version,
                target_version.updated_at.with_timezone(&chrono::Local)
            ))
            .bold(),
        );

        client
            .post("/stack/rollback")
            .json(&stack::rollback::Request {
                name: project.name.to_string(),
                version: self.command.version,
            })
            .send()
            .await
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        let mut status = project
            .status()
            .await
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        // Poll the status of the rollback
        while status.status == "IN_PROGRESS" {
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            status = project
                .status()
                .await
                .map_err(|e| self.error(None, None, Some(e.into())))?;
        }

        if status.status == "FAILED" {
            return Err(self.error(
                Some("Rollback failed"),
                Some("Try again in a few seconds."),
                None,
            ));
        }

        println!("{}", console::style("Done").green());
        Ok(())
    }
}
