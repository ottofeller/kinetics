pub mod pipeline;
use crate::project::Project;
use eyre::Context;
use pipeline::Pipeline;

/// The entry point to run the command
pub async fn run(deploy_functions: &[String]) -> eyre::Result<()> {
    Pipeline::builder()
        .with_deploy_enabled(false)
        .set_project(Project::from_current_dir()?)
        .build()
        .wrap_err("Failed to build pipeline")?
        .run(deploy_functions)
        .await?;

    Ok(())
}
