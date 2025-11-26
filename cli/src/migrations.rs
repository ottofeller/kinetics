use color_eyre::owo_colors::OwoColorize;
use eyre::Context;
use sqlx::Row;
use std::path::Path;

/// A struct representing a set of database migration files.
///
/// The `Migrations` struct is used to manage and refer to a collection of database
/// migration files stored in a specific directory.
pub struct Migrations<'a> {
    /// Directory where migration files are stored
    path: &'a Path,
}

/// Methods for managing database migrations
impl<'a> Migrations<'a> {
    /// Creates a new `Migrations` instance from a directory path
    ///
    /// `path` is a path to the migrations directory; it must exist in the filesystem
    pub fn new(path: &'a Path) -> eyre::Result<Self> {
        if !path.try_exists()? {
            return Err(eyre::eyre!(
                "Migrations directory does not exist: {}",
                path.display()
            ));
        }

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

        let connection = sqlx::PgPool::connect(&connection_string)
            .await
            .wrap_err("Failed to connect to database")?;

        // Get latest applied migration
        let result = sqlx::query("SELECT MAX(id) FROM schema_migrations")
            .fetch_one(&connection)
            .await?;

        let last_db_id: String = result
            .try_get::<Option<String>, _>(0)
            .unwrap_or_default()
            .unwrap_or("0".to_string());

        let migrations = self.migrations(&last_db_id).await?;

        if migrations.is_empty() {
            println!("{}", console::style("No migrations to apply...").yellow());
            return Ok(());
        }

        let mut tx = connection.begin().await?;

        for (filename, content) in migrations {
            sqlx::raw_sql(&content)
                .execute(&mut *tx)
                .await
                .inspect_err(|e| log::error!("Error: {e:?}"))
                .wrap_err("Failed to apply migration")?;

            sqlx::query(r#"INSERT INTO "schema_migrations" (id) VALUES ($1)"#)
                .bind(&filename)
                .execute(&mut *tx)
                .await?;

            println!(
                "{}: {}",
                console::style("Successfully applied").green(),
                console::style(&filename).dimmed()
            );
        }

        println!(
            "{}",
            console::style("All migrations were applied").green().bold()
        );

        tx.commit().await?;
        Ok(())
    }

    /// Creates a new migration file with a unique filename based on the current
    /// timestamp and an optional user-provided name.
    ///
    /// The migration file is created within a specified directory and is initially empty.
    pub async fn create(&self, name: Option<&str>) -> eyre::Result<()> {
        let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");

        // Allow only alphanumeric characters and underscores
        let name = name
            .unwrap_or_default()
            .replace(" ", "_")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .take(100)
            .collect::<String>();

        // Generate a unique filename based on the current timestamp and optional migration name
        let filename = [timestamp.to_string(), name]
            .into_iter()
            .filter(|c| !c.is_empty())
            .collect::<Vec<_>>()
            .join("_");

        let filepath = self.path.join(format!("{}.up.sql", filename));

        // TODO Add some helpful comments to the migration file
        tokio::fs::write(&filepath, "")
            .await
            .wrap_err("Failed to create a migration file")?;

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

    /// Retrieves a sorted (ASC) list of database migration files and their contents to be applied.
    ///
    /// `last_applied_id`: representing the identifier of the last applied migration.
    /// Only migration files with names greater than this identifier (in lexicographical order)
    /// will be included.
    ///
    /// Returns a vector of tuples. Each tuple contains:
    ///   - `String`: The name of the migration file
    ///   - `String`: The content of the migration file
    async fn migrations(&self, last_applied_id: &str) -> eyre::Result<Vec<(String, String)>> {
        let mut read_dir = tokio::fs::read_dir(self.path)
            .await
            .wrap_err("Failed to read migrations dir")?;

        let mut paths = Vec::new();

        // Collect all valid migration files
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let filename = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_owned(),
                None => {
                    log::warn!("Invalid filename: {:?}. Skipping...", path);
                    continue;
                }
            };

            if filename.ends_with(".up.sql") {
                paths.push((filename, path));
            }
        }

        // Sort migrations by name in ASC (the oldest first) order
        paths.sort_by(|(name1, _), (name2, _)| name1.cmp(name2));

        let mut result = Vec::new();

        // Filter out migrations that have already been applied and read its content
        for (filename, path) in paths {
            if filename.as_str() > last_applied_id {
                let content = tokio::fs::read_to_string(path)
                    .await
                    .wrap_err("Failed to read file")?;

                result.push((filename, content));
            }
        }

        Ok(result)
    }
}
