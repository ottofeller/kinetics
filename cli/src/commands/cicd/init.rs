use crate::{commands::cicd::github, project::Project};

/// Initialize a deployment workflow within an existing kinetics project.
pub async fn init(project: &Project, github: bool) -> eyre::Result<()> {
    if !github {
        println!(
            "{}",
            console::style("No CI/CD provider selected. Use github.").yellow()
        );
    }

    github::workflow(project)?;

    println!("{}", console::style("Done").bold().green());
    Ok(())
}
