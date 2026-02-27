use crate::error::Error;
use crate::migrations::Migrations;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;

#[derive(clap::Args, Clone)]
pub(crate) struct CreateCommand {
    /// User-defined name for the migration
    #[arg(value_name = "NAME")]
    name: Option<String>,

    /// Relative path to migrations directory
    #[arg(short, long, value_name = "PATH", default_value = "migrations")]
    path: String,
}

impl Runnable for CreateCommand {
    fn runner(&self, _writer: &Writer) -> impl Runner {
        CreateRunner {
            command: self.clone(),
        }
    }
}

struct CreateRunner {
    command: CreateCommand,
}

impl Runner for CreateRunner {
    /// Creates a new database migration file
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;
        let migrations_path = project.path.join(&self.command.path);

        // Create migrations directory if it doesn't exist
        tokio::fs::create_dir_all(&migrations_path)
            .await
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        Migrations::new(&migrations_path)
            .map_err(|e| self.error(None, None, Some(e.into())))?
            .create(self.command.name.as_deref())
            .await
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        Ok(())
    }
}
