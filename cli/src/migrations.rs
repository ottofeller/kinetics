use color_eyre::owo_colors::OwoColorize;
use eyre::Context;
use sqlx::{Pool, Postgres, Row};
use std::path::Path;

pub struct Migrations<'a> {
    path: &'a Path,
}

/// Methods for managing database migrations
impl<'a> Migrations<'a> {
    pub async fn new(path: &'a Path) -> eyre::Result<Self> {
        tokio::fs::create_dir_all(path)
            .await
            .wrap_err("Failed to create migrations dir")?;

        Ok(Self { path })
    }

    /// Applies database migrations based on the stored migration files and the current state
    /// of the database.
    ///
    /// This function retrieves the latest applied migration ID, and determines which migrations
    /// (if any) need to be applied. It then applies the pending migrations sequentially
    /// and updates the `schema_migrations` table to record each migration.
    pub async fn apply(&self, connection_string: String) -> eyre::Result<()> {
        println!("{}", console::style("Applying migrations...").dimmed());

        let all_migrations = self.migrations().await?;

        let connection = sqlx::PgPool::connect(&connection_string)
            .await
            .wrap_err("Failed to connect to database")?;

        self.ensure_migrations_table(&connection)
            .await
            .wrap_err("Failed to fetch migrations table")?;

        // Get latest applied migration
        let result = sqlx::query("SELECT MAX(id) FROM schema_migrations")
            .fetch_one(&connection)
            .await?;

        let last_db_id: String = result
            .try_get::<Option<String>, _>(0)
            .unwrap_or_default()
            .unwrap_or("0".to_string());

        // Get latest migration file name
        let last_file_id = all_migrations.last().cloned().unwrap_or("0".to_string());

        // Check if there are migrations to apply
        if last_db_id >= last_file_id || all_migrations.is_empty() {
            println!(
                "{}",
                console::style("No migrations to apply...").red().bold()
            );
            return Ok(());
        }

        let mut tx = connection.begin().await?;

        for migration in all_migrations {
            if migration <= last_db_id {
                continue;
            }

            let sql = tokio::fs::read_to_string(&self.path.join(&migration))
                .await
                .wrap_err("Failed to read migration file")?;

            sqlx::query(&sql)
                .execute(&mut *tx)
                .await
                .wrap_err("Failed to apply migration")?;

            println!(
                "{}: {}",
                console::style("Done").green(),
                console::style(&migration).dimmed()
            );

            sqlx::query("INSERT INTO schema_migrations (id) VALUES ($1)")
                .bind(&migration)
                .execute(&mut *tx)
                .await?;
        }

        println!("{}", console::style("All migrations were applied").green());
        tx.commit().await?;
        Ok(())
    }

    pub async fn create(&self, name: &str) -> eyre::Result<()> {
        let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");

        // Allow only alphanumeric characters and underscores
        let name = name
            .replace(" ", "_")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .take(100)
            .collect::<String>();

        let filepath = self.path.join(format!("{}_{}.up.sql", timestamp, name));

        // TODO Add some helpful comments to the migration file
        tokio::fs::write(&filepath, "")
            .await
            .wrap_err("Failed to create migration file")?;

        println!(
            "{}: {}",
            console::style("Migration created successfully")
                .green()
                .bold(),
            console::style(format!(
                "{}/{}",
                filepath
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default(),
                filepath
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            ))
            .dimmed(),
        );

        Ok(())
    }

    /// Retrieves the list of migration files from the specified directory.
    ///
    /// It returns only file names that end with `.up.sql`.
    /// Files ordered by names in ASC (oldest first) order
    async fn migrations(&self) -> eyre::Result<Vec<String>> {
        let mut read_dir = tokio::fs::read_dir(self.path).await?;
        let mut entries = Vec::new();

        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".up.sql") {
                    entries.push(name.to_string());
                }
            }
        }

        // Sort files by name (oldest first)
        entries.sort();
        Ok(entries)
    }

    /// Ensures that the migrations table exists in the database and creates it if it doesn't
    async fn ensure_migrations_table(&self, connection: &Pool<Postgres>) -> eyre::Result<()> {
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
