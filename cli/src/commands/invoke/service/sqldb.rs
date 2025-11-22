use sqlx::PgPool;

const DOCKER_COMPOSE_SNIPPET: &str = r#"
local-postgres:
    image: "postgres:16.10"
    shm_size: 128mb
    ports:
        - "5432:5432"
    volumes:
        - "/tmp/postgres:/var/lib/postgresql/data"
    environment:
      POSTGRES_PASSWORD: localdbpassword
"#;

pub struct LocalSqlDB {}

impl LocalSqlDB {
    pub fn new() -> Self {
        Self {}
    }

    pub fn docker_compose_snippet(&self) -> String {
        DOCKER_COMPOSE_SNIPPET.to_string()
    }

    pub fn connection_string(&self) -> String {
        // Be careful with password, change it in `DOCKER_COMPOSE_SNIPPET` accordingly
        "postgres://postgres:localdbpassword@localhost:5432/postgres?sslmode=disable".to_string()
    }

    /// Attempts to provision a PostgreSQL connection, retrying on failure.
    pub async fn provision(&self) -> eyre::Result<()> {
        let max_retries = 5;
        let retry_delay_ms = 1000;

        for attempt in 1..=max_retries {
            let result = PgPool::connect(&self.connection_string()).await;

            match result {
                Ok(_) => return Ok(()),
                Err(_) if attempt < max_retries => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms)).await;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}
