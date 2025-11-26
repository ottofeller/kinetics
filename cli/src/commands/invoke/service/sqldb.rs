use crate::migrations::Migrations;
use crate::project::Project;
use eyre::Context;
use sqlx::{PgPool, Pool, Postgres};
use std::path::PathBuf;

const DOCKER_COMPOSE_SNIPPET: &str = r#"
local-postgres:
    image: "postgres:16.10"
    shm_size: 128mb
    ports:
        - "5432:5432"
    volumes:
        - "{{DB_VOLUME_PATH}}:/var/lib/postgresql/data"
    environment:
      POSTGRES_PASSWORD: localdbpassword
"#;

/// A structure representing a local SQL database configuration.
///
/// This struct is used to configure properties for setting up a local SQL database.
pub struct LocalSqlDB {
    /// Current project
    project: Project,

    /// Whether to apply database migrations on startup
    with_migrations: bool,

    /// Full path to the migrations directory
    ///
    /// Default is <project >/migrations/
    migrations_path: PathBuf,
}

impl LocalSqlDB {
    pub fn new(project: &Project) -> Self {
        Self {
            // The default migrations path is `migrations` relative to the project root directory
            migrations_path: project.path.join("migrations"),
            with_migrations: false,
            project: project.clone(),
        }
    }

    pub fn docker_compose_snippet(&self) -> String {
        DOCKER_COMPOSE_SNIPPET
            .replace(
                "{{DB_VOLUME_PATH}}",
                format!("/tmp/kinetics_db_{}", self.project.name).as_str(),
            )
            .to_string()
    }

    pub fn connection_string(&self) -> String {
        // Be careful with password, change it in `DOCKER_COMPOSE_SNIPPET` accordingly
        "postgres://postgres:localdbpassword@localhost:5432/postgres?sslmode=disable".to_string()
    }

    /// Sets whether to apply database migrations on startup
    ///
    /// `migrations_path` is relative to the project root directory
    pub fn with_migrations(&mut self, migrations_path: Option<&str>) -> &mut Self {
        self.with_migrations = true;

        // Use a migrations path is specified; otherwise, the default migrations path will be used
        if let Some(migrations_path) = migrations_path {
            self.migrations_path = self.project.path.join(migrations_path);
        }

        self
    }

    /// Attempts to provision a PostgreSQL connection, retrying on failure.
    pub async fn provision(&self) -> eyre::Result<()> {
        let max_retries = 10;
        let retry_delay_ms = 1000;

        for attempt in 1..=max_retries {
            let result = PgPool::connect(&self.connection_string()).await;

            match result {
                Ok(connection) => {
                    self.cleanup(&connection).await?;
                    self.create_migrations_table(&connection).await?;
                    break; // Connection successful, exit the loop
                }
                Err(_) if attempt < max_retries => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms)).await;
                }
                Err(e) => return Err(e.into()),
            }
        }

        if self.with_migrations {
            Migrations::new(self.migrations_path.as_path())?
                .apply(self.connection_string())
                .await?;
        }

        Ok(())
    }

    /// Cleans the database up by dropping all tables
    async fn cleanup(&self, connection: &Pool<Postgres>) -> eyre::Result<()> {
        sqlx::raw_sql(
            r#"
            DROP SCHEMA public CASCADE;
            CREATE SCHEMA public;
            GRANT ALL ON SCHEMA public TO postgres;
            GRANT ALL ON SCHEMA public TO public;
        "#,
        )
        .execute(connection)
        .await
        .wrap_err("Failed to clean database")?;

        Ok(())
    }

    /// Creates the `schema_migrations` table if it doesn't exist.
    ///
    /// This table is used to track database schema migrations.
    /// See [Migrations](crate::migrations::Migrations) for more details.
    async fn create_migrations_table(&self, connection: &Pool<Postgres>) -> eyre::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                id VARCHAR(255) PRIMARY KEY,
                applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )
        .execute(connection)
        .await
        .wrap_err("Failed to create migrations table")?;

        Ok(())
    }
}
