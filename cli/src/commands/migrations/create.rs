use crate::migrations::Migrations;
use crate::project::Project;
use eyre::Context;

pub async fn create(project: &Project, path: Option<&str>, name: Option<&str>) -> eyre::Result<()> {
    Migrations::new(project.path.join(path.unwrap_or("migrations")).as_path())
        .await
        .wrap_err("Failed to initialize migrations")?
        .create(name)
        .await
        .wrap_err("Failed to create new migration")?;

    Ok(())
}
