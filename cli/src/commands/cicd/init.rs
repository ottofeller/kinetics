use crate::{commands::cicd::github::github_workflow, project::Project};

/// Initialize a new Kinetics project by downloading and unpacking a template archive
///
/// Downloads the Kinetics template archive into a new directory,
/// customizes it with the provided project name, and sets up a ready-to-use project structure.
pub async fn init(project: &Project, github: bool) -> eyre::Result<()> {
    if github {
        github_workflow(project)?;
    } else {
        println!("{}", console::style("No CI/CD provider selected").dim());
    }

    println!("{}", console::style("Done").bold().green());
    Ok(())
}
