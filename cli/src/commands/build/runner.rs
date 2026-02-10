use crate::commands::build::pipeline::Pipeline;
use crate::commands::build::BuildCommand;
use crate::error::Error;
use crate::runner::Runner;
use eyre::Context;

pub(crate) struct BuildRunner {
    pub(crate) command: BuildCommand,
}

impl Runner for BuildRunner {
    /// Build one or more functions
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        Pipeline::builder()
            .with_deploy_enabled(false)
            .set_project(project)
            .build()
            .wrap_err("Failed to build pipeline")?
            .run(&self.command.functions)
            .await?;

        Ok(())
    }
}
