use crate::error::Error;
use crate::migrations::Migrations;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use serde_json::json;
use std::path::PathBuf;

#[derive(clap::Args, Clone)]
pub(crate) struct CreateCommand {
    /// User-defined name for the migration
    #[arg(value_name = "NAME")]
    name: Option<String>,

    /// Relative path to migrations directory
    #[arg(short, long, value_name = "PATH", default_value = "migrations")]
    path: String,

    /// Relative path to the project directory
    #[arg(long)]
    project: Option<PathBuf>,
}

impl Runnable for CreateCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        CreateRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct CreateRunner<'a> {
    command: CreateCommand,
    writer: &'a Writer,
}

impl Runner for CreateRunner<'_> {
    /// Creates a new database migration file
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project(&self.command.project).await?;
        let migrations_path = project.path.join(&self.command.path);

        // Create migrations directory if it doesn't exist
        tokio::fs::create_dir_all(&migrations_path)
            .await
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        Migrations::new(&migrations_path, self.writer)
            .map_err(|e| self.error(None, None, Some(e.into())))?
            .create(self.command.name.as_deref())
            .await
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        self.writer.json(json!({"success": true}))?;
        Ok(())
    }
}
