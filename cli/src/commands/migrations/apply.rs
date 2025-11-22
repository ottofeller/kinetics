// FIXME Move from the commands namespace
use crate::commands::invoke::{docker::Docker, service::LocalSqlDB};
use crate::config::build_config;
use crate::migrations::Migrations;
use eyre::Context;
use std::path::PathBuf;

/// Apply migrations to the local SQL DB
pub async fn apply(path: Option<&str>) -> eyre::Result<()> {
    let current_dir = std::env::current_dir().wrap_err("Failed to get current dir")?;

    // Run local SQL DB
    let mut docker = Docker::new(&PathBuf::from(&build_config()?.kinetics_path));
    let sqldb = LocalSqlDB::new();
    let db_connection_string = sqldb.connection_string();

    // Run docker containers for SQL DB
    docker.with_sqldb(sqldb);
    docker.start()?;

    // After provisioning is up and running, we can apply migrations
    docker.provision().await?;

    Migrations::new(current_dir.join(path.unwrap_or("migrations")).as_path())
        .await
        .wrap_err("Failed to initialize migrations")?
        .apply(db_connection_string)
        .await
        .wrap_err("Failed to apply migrations")?;

    Ok(())
}
