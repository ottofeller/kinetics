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
}
