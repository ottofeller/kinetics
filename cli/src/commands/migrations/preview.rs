use crate::error::Error;
use crate::migrations::Migrations;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;
use kinetics_api::project;
use project::sqldb::connect::Request;
use serde_json::json;
use std::path::PathBuf;

#[derive(clap::Args, Clone)]
pub(crate) struct PreviewCommand {
    /// Relative path to migrations directory
    #[arg(short, long, value_name = "PATH", default_value = "migrations")]
    path: String,

    /// Relative path to the project directory
    #[arg(long)]
    project: Option<PathBuf>,

    /// Name of the org to preview migrations in
    #[arg(long)]
    org: Option<String>,
}

impl Runnable for PreviewCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        PreviewRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct PreviewRunner<'a> {
    command: PreviewCommand,
    writer: &'a Writer,
}

impl<'a> Runner for PreviewRunner<'a> {
    /// Lists migrations that are not yet applied to the database
    async fn run(&mut self) -> Result<(), Error> {
        let mut project = self.project(&self.command.project).await?;
        let client = self.api_client().await?;
        let migrations_path = project.path.join(&self.command.path);

        if self.command.org.is_some() {
            project = project.with_org(self.command.org.as_deref());
        }

        self.writer.text(&format!(
            "{} migrations {} {}...\n\n",
            console::style("Previewing").bold(),
            console::style("from").dim(),
            console::style(format!("{}", migrations_path.to_string_lossy())).underlined(),
        ))?;

        let response = client
            .request::<_, project::sqldb::connect::Response>(
                "/stack/sqldb/connect",
                Request {
                    project: project.into(),
                },
            )
            .await
            .wrap_err("Failed to get SQL DB connection string")
            .map_err(|e| self.server_error(Some(e.into())))?;

        let migrations = Migrations::new(migrations_path.as_path(), self.writer)
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        let filenames = migrations
            .preview(response.connection_string)
            .await
            .wrap_err("Failed to preview migrations")
            .map_err(|e| self.server_error(Some(e.into())))?;

        self.writer
            .text(&format!("\n{}\n", console::style("Done").bold()))?;

        self.writer
            .json(json!({"success": true, "migrations": filenames}))?;
        Ok(())
    }
}
