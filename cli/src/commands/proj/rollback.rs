use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;
use kinetics_api::stack;
use serde_json::json;
use std::path::PathBuf;

#[derive(clap::Args, Clone)]
pub(crate) struct RollbackCommand {
    /// Specific version to rollback to (optional)
    #[arg(short, long)]
    version: Option<u32>,

    /// Relative path to the project directory
    #[arg(long)]
    project: Option<PathBuf>,
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
        let project = self.project(&self.command.project).await?;
        let client = self.api_client().await?;

        self.writer.text(&format!(
            "{} {}...\n\n",
            console::style("Rolling back").bold().green(),
            match self.command.version {
                Some(version) => format!("to version {version}"),
                None => "to the previous version".to_owned(),
            },
        ))?;

        client
            .request::<_, stack::rollback::Response>(
                "/stack/rollback",
                stack::rollback::Request {
                    project: (&project).into(),
                    // None asks the backend to choose the previous version.
                    version: self.command.version,
                },
            )
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

        if status.status != "COMPLETE" {
            return Err(Error::stack_operation(&status, "Rollback failed"));
        }

        self.writer
            .text(&format!("{}\n", console::style("Done").bold()))?;

        self.writer.json(json!({"success": true}))?;
        Ok(())
    }
}
