use crate::api::project::sqldb::{Request as ConnectRequest, Response as ConnectResponse};
use crate::client::Client;
use crate::migrations::Migrations;
use crate::project::Project;
use eyre::Context;

/// Applies migrations to the database
///
/// `project` – current project
/// `migrations_dir` – relative to project.path directory name
pub async fn apply(project: &Project, migrations_dir: &str) -> eyre::Result<()> {
    let migrations_path = project.path.join(migrations_dir);

    println!(
        "{}",
        console::style("Applying migrations...").green().bold()
    );

    let response = Client::new(false)
        .await?
        .request::<_, ConnectResponse>(
            "/stack/sqldb/connect",
            ConnectRequest {
                project: project.name.clone(),
            },
        )
        .await
        .wrap_err("Failed to get SQL DB connection string")?;

    // FIXME Move create migrations table routine
    let connection = sqlx::PgPool::connect(&response.connection_string).await?;

    sqlx::query(
        r#"
             CREATE TABLE IF NOT EXISTS schema_migrations (
                id VARCHAR(255) PRIMARY KEY,
                applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
        "#,
    )
    .execute(&connection)
    .await?;

    let migrations = Migrations::new(migrations_path.as_path())?;
    migrations.apply(response.connection_string).await?;

    println!(
        "{}\n",
        console::style("Migrations applied successfully")
            .green()
            .bold()
    );

    Ok(())
}
