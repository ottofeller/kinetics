use eyre::{Context, ContextCompat};
use serde_json::Value;
use sqlx::postgres::{PgArguments, PgPoolOptions};
use sqlx::{Arguments, Pool, Postgres, Row};
use std::collections::HashMap;
use std::path::Path;

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

pub struct LocalSqlDB {
    /// List of fixture files to load into the database
    fixtures: Vec<String>,
}

impl LocalSqlDB {
    pub fn new() -> Self {
        Self { fixtures: vec![] }
    }

    pub fn with_fixtures(mut self, fixtures: &[String]) -> Self {
        self.fixtures = Vec::from(fixtures);
        self
    }

    pub fn docker_compose_snippet(&self) -> String {
        DOCKER_COMPOSE_SNIPPET.to_string()
    }

    pub fn connection_string(&self) -> String {
        // Be careful with password, change it in `DOCKER_COMPOSE_SNIPPET` accordingly
        "postgres://postgres:localdbpassword@localhost:5432/postgres?sslmode=disable".to_string()
    }

    /// Provision database with a retry mechanism for handling connection issues
    /// If fixtures are provided, the method tries to connect to the database and load fixtures.
    pub async fn provision(&self) -> eyre::Result<()> {
        // Do nothing if there are no fixtures to load
        if self.fixtures.is_empty() {
            return Ok(());
        }

        let max_retries = 5;
        let retry_delay_ms = 1000;

        for attempt in 1..=max_retries {
            // Retry only if the connection fails
            let fixtures = Fixtures::new(self.connection_string(), &self.fixtures).await;

            match fixtures {
                Ok(fixtures) => {
                    fixtures.apply().await.wrap_err("Failed to load fixtures")?;
                    return Ok(());
                }
                Err(err) => {
                    if attempt == max_retries {
                        return Err(err);
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(retry_delay_ms)).await;
                }
            }
        }

        Ok(())
    }
}

/// A structure representing SQL database fixtures used for setting up or interacting
/// with a database during testing or other operations.
struct Fixtures<'a> {
    /// Connection pool to the database
    connections_pool: Pool<Postgres>,

    /// List of fixture files to load into the database
    fixtures: &'a [String],
}

impl<'a> Fixtures<'a> {
    pub async fn new(connection_string: String, fixtures: &'a [String]) -> eyre::Result<Self> {
        Ok(Self {
            connections_pool: PgPoolOptions::new()
                .connect(&connection_string)
                .await
                .wrap_err("Filed to connect to database")?,

            fixtures,
        })
    }

    /// Apply all fixtures into the database
    pub async fn apply(&self) -> eyre::Result<()> {
        let current_dir = std::env::current_dir().wrap_err("Failed to get current directory")?;
        let fixtures_dir = current_dir.join("fixtures");

        for fixture in self.fixtures {
            let file_path = fixtures_dir.join(fixture);
            self.load(&file_path).await?;
        }

        Ok(())
    }

    /// Loads fixtures from provided `file_path` into the database
    ///
    /// Each fixture is a JSON object with table names as keys and arrays of rows as values
    /// Each row is a JSON object with column names as keys and column values as values
    ///
    /// File format
    /// ```json
    /// {
    ///   "table_name": [
    ///     {"field1": "value", "field2": "value"},
    ///   ]
    /// }
    /// ```
    async fn load(&self, file_path: &Path) -> eyre::Result<()> {
        let content = tokio::fs::read_to_string(file_path)
            .await
            .wrap_err("Failed to read fixture")?;

        let json_value: Value =
            serde_json::from_str(&content).wrap_err("Fixtures JSON is invalid")?;

        let object = json_value
            .as_object()
            .wrap_err("Fixture file must be a JSON object \"{<table name>: [ {<column name>: <column value>} ]}\"")?;

        // All inserts from fixtures are executed in a single transaction
        let mut tx = self
            .connections_pool
            .begin()
            .await
            .wrap_err("Failed to start transaction")?;

        for (table, rows_value) in object {
            let rows_arr = rows_value.as_array().wrap_err("Wrong fixture format")?;

            // Fetch column types for each table
            let column_types = self.fetch_column_types(table).await?;

            for row_value in rows_arr {
                let obj = row_value
                    .as_object()
                    .wrap_err("Fixture rows must be objects")?;

                // Prepare a column list for query
                // INSERT INTO "tbl" ("c1","c2") VALUES ($1::type1, $2::type2)
                let mut cols: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
                cols.sort(); // Sort columns for determinism
                log::debug!("Got columns for table {}: {:?}", table, cols);

                let mut args = PgArguments::default();
                let mut placeholders = Vec::with_capacity(cols.len());

                let columns_repr = cols
                    .iter()
                    .map(|c| Self::quote_ident(c))
                    .collect::<Vec<_>>()
                    .join(", ");

                for (idx, name) in cols.iter().enumerate() {
                    let pg_type = column_types
                        .get(*name)
                        .wrap_err("Column isn't found in the table")?;

                    // Prepare a list of placeholders for a query
                    placeholders.push(format!("${}::{}", idx + 1, pg_type));

                    // Convert each column value to a string suitable for SQL arguments
                    let string_value =
                        Self::value_to_string(obj.get(*name).unwrap_or(&Value::Null));

                    if let Err(err) = args.add(string_value) {
                        return Err(eyre::eyre!("Failed to add argument: {}", err));
                    }
                }

                let sql = format!(
                    "INSERT INTO {table} ({columns}) VALUES ({values})",
                    table = Self::quote_ident(table),
                    columns = columns_repr,
                    values = placeholders.join(", ")
                );

                sqlx::query_with(&sql, args)
                    .execute(&mut *tx)
                    .await
                    .wrap_err(format!("Failed to insert data into table {}", table))?;
            }
        }

        tx.commit().await.wrap_err("Failed to commit transaction")?;
        Ok(())
    }

    /// Fetches column types from the provided `table`
    /// Returns a hashmap where `key` is the column name and `value` is the PostgreSQL type
    async fn fetch_column_types(&self, table: &str) -> eyre::Result<HashMap<String, String>> {
        let rows = sqlx::query(
            r#"
            SELECT a.attname AS column_name,
                   format_type(a.atttypid, a.atttypmod) AS type_sql
            FROM pg_attribute a
            WHERE a.attrelid = to_regclass($1) -- resolves table names using the search_path
              AND a.attnum > 0
              AND NOT a.attisdropped
            ORDER BY a.attnum
        "#,
        )
        .bind(table)
        .fetch_all(&self.connections_pool)
        .await?;

        Ok(HashMap::from_iter(
            rows.iter()
                .map(|r| (r.get("column_name"), r.get("type_sql"))),
        ))
    }

    /// Converts provided `Value` to a string suitable for SQL arguments
    /// Returns `None` if the value is JSON Null, which sqlx handles correctly as SQL NULL
    /// For example, `Value::Bool(true)` becomes `"true"`.
    fn value_to_string(value: &Value) -> Option<String> {
        match value {
            Value::Null => None,
            Value::Bool(b) => Some(b.to_string()),
            Value::Number(n) => Some(n.to_string()),
            Value::String(s) => Some(s.clone()),
            Value::Array(_) | Value::Object(_) => Some(serde_json::to_string(value).unwrap()),
        }
    }

    /// Quotes a SQL identifier by wrapping it in double quotes.
    fn quote_ident(id: &str) -> String {
        format!("\"{}\"", id.replace('"', "\"\""))
    }
}
