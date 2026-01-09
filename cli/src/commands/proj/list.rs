use crate::error::Error;
use crate::project::Project;
use color_eyre::owo_colors::OwoColorize;

/// Prints out the list of all projects
pub async fn list() -> Result<(), Error> {
    Project::fetch_all()
        .await?
        .iter()
        .for_each(|Project { name, url, .. }| println!("{}\n{}\n\n", name.bold(), url.dimmed()));

    Ok(())
}
