use crate::migrations::Migrations;
use sqlx::PgPool;
use std::path::PathBuf;

const DOCKER_COMPOSE_SNIPPET: &str = r#"
local-postgres:
    image: "postgres:16.10"
    shm_size: 128mb
    ports:
        - "5432:5432"
    environment:
      POSTGRES_PASSWORD: localdbpassword
"#;

pub struct LocalSqlDB {
    /// Path to the migrations directory, if specified migrations will be applied on startup
    with_migrations: Option<PathBuf>,
}

impl LocalSqlDB {
    pub fn new() -> Self {
        Self {
            with_migrations: None,
        }
    }

    pub fn docker_compose_snippet(&self) -> String {
        DOCKER_COMPOSE_SNIPPET.to_string()
    }

    pub fn connection_string(&self) -> String {
        // Be careful with password, change it in `DOCKER_COMPOSE_SNIPPET` accordingly
        "postgres://postgres:localdbpassword@localhost:5432/postgres?sslmode=disable".to_string()
    }

    pub fn with_migrations(&mut self, with_migrations: Option<PathBuf>) -> &mut Self {
        self.with_migrations = with_migrations;
        self
    }

    /// Attempts to provision a PostgreSQL connection, retrying on failure.
    pub async fn provision(&self) -> eyre::Result<()> {
        let max_retries = 5;
        let retry_delay_ms = 1000;

        for attempt in 1..=max_retries {
            let result = PgPool::connect(&self.connection_string()).await;

            match result {
                Ok(_) => break, // Connection successful, exit the loop
                Err(_) if attempt < max_retries => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms)).await;
                }
                Err(e) => return Err(e.into()),
            }
        }

        if let Some(with_migrations) = &self.with_migrations {
            Migrations::new(with_migrations.as_path())
                .await?
                .apply(self.connection_string())
                .await?;
        }

        Ok(())
    }
}
