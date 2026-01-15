use crate::{commands::cicd::github, project::Project};

/// Initialize a deployment workflow within an existing kinetics project
///
/// Currently only supports GitHub, but expected to expand in the future.
pub async fn init(project: &Project, _github: bool) -> eyre::Result<()> {
    println!(
        "{}",
        console::style("Creating GitHub workflow...").bold().green()
    );

    github::workflow(project, false)?;
    println!("{}", console::style("Done").bold().green());
    Ok(())
}
