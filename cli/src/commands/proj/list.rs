use crate::error::Error;
use crate::project::Project;
use color_eyre::owo_colors::OwoColorize;
use crossterm::style::Stylize;
use eyre::Context;

/// Prints out the list of all projects
pub async fn list() -> Result<(), Error> {
    Project::all()
        .await
        .inspect_err(|e| log::error!("Failed to load list of projects: {e}"))
        .wrap_err("Request failed. Tray again later.")?
        .iter()
        .for_each(|p| println!("{}\n{}\n\n", p.name.clone().bold(), p.url.dimmed()));

    Ok(())
}
