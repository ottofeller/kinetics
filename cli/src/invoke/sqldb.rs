const DOCKER_COMPOSE_SERVICE: &str = r#"
local-postgres:
    image: "postgres:16"
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

    pub fn docker_compose_service(&self) -> &str {
        DOCKER_COMPOSE_SERVICE
    }

    pub fn connection_string(&self) -> String {
        // Be careful with password, change it in DOCKER_COMPOSE_SERVICE accordingly
        "postgres://postgres:localdbpassword@localhost:5432/postgres?sslmode=disable".to_string()
    }
}
