use crate::migration::Migration;
use eyre::Context;

pub async fn create(path: Option<&str>, name: &str) -> eyre::Result<()> {
    let current_dir = std::env::current_dir().wrap_err("Failed to get current dir")?;

    Migration::new(current_dir.join(path.unwrap_or("migrations")).as_path())
        .await
        .wrap_err("Failed to initialize migrations")?
        .create(name)
        .await
        .wrap_err("Failed to create new migration")?;

    Ok(())
}
