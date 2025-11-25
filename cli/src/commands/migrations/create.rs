use crate::migrations::Migrations;
use crate::project::Project;
use eyre::Context;

/// Creates a new database migration file
///
/// `project` – current project
/// `migrations_dir` – relative to project.path directory name
///     - Defaults to `migrations` – it will be created if it doesn't exist
///     - If set to Some(...), it must be a relative to `project.path` and must exist
/// `name` – optional migration name
pub async fn create(
    project: &Project,
    migrations_dir: Option<&str>,
    name: Option<&str>,
) -> eyre::Result<()> {
    let dir_name = migrations_dir.unwrap_or("migrations");
    let migrations_path = project.path.join(dir_name);

    // Create migrations directory if it doesn't exist
    tokio::fs::create_dir_all(&migrations_path).await?;

    Migrations::new(&migrations_path)
        .wrap_err("Failed to initialize migrations")?
        .create(name)
        .await
        .wrap_err("Failed to create new migration")?;

    Ok(())
}
