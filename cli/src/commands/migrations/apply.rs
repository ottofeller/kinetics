use crate::api::project;
use crate::error::Error;
use crate::migrations::Migrations;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;
use project::sqldb::connect::Request;

#[derive(clap::Args, Clone)]
pub(crate) struct ApplyCommand {
    /// Relative path to migrations directory
    #[arg(short, long, value_name = "PATH", default_value = "migrations")]
    path: String,
}

impl Runnable for ApplyCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        ApplyRunner {
            command: self.clone(),
        }
    }
}

struct ApplyRunner {
    command: ApplyCommand,
}

impl Runner for ApplyRunner {
    /// Applies migrations to the database
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;
        let client = self.api_client().await?;
        let migrations_path = project.path.join(&self.command.path);

        println!(
            "{} {} {}...",
            console::style("Applying migrations").green().bold(),
            console::style("from").dim(),
            console::style(format!("{}", migrations_path.to_string_lossy()))
                .underlined()
                .bold(),
        );

        let response = client
            .request::<_, project::sqldb::connect::Response>(
                "/stack/sqldb/connect",
                Request {
                    project: project.name.clone(),
                },
            )
            .await
            .wrap_err("Failed to get SQL DB connection string")
            .map_err(|e| self.server_error(Some(e.into())))?;

        // FIXME Move create migrations table routine
        let connection = sqlx::PgPool::connect(&response.connection_string)
            .await
            .map_err(|e| self.server_error(Some(e.into())))?;

        sqlx::query(
            r#"
             CREATE TABLE IF NOT EXISTS schema_migrations (
                id VARCHAR(255) PRIMARY KEY,
                applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
        "#,
        )
        .execute(&connection)
        .await
        .map_err(|e| self.server_error(Some(e.into())))?;

        let migrations = Migrations::new(migrations_path.as_path())
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        migrations
            .apply(response.connection_string)
            .await
            .wrap_err("Failed to apply migrations")
            .map_err(|e| self.server_error(Some(e.into())))?;

        println!("\n{}\n", console::style("Done").green().bold());
        Ok(())
    }
}
