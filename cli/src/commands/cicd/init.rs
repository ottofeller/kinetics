use crate::{commands::cicd::github, project::Project};

/// Initialize a deployment workflow within an existing kinetics project.
pub async fn init(project: &Project, github: bool) -> eyre::Result<()> {
    println!(
        "{}",
        console::style("Adding GitHub workflow...").yellow()
    );
    
    github::workflow(project)?;
    println!("{}", console::style("Done").bold().green());
    Ok(())
}
