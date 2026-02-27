use crate::commands::build::pipeline::Pipeline;
use crate::commands::build::BuildCommand;
use crate::error::Error;
use crate::runner::Runner;
use crate::writer::Writer;
use eyre::Context;

pub(crate) struct BuildRunner<'a> {
    pub(crate) command: BuildCommand,
    pub(crate) writer: &'a Writer,
}

impl Runner for BuildRunner<'_> {
    /// Build one or more functions
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        Pipeline::builder(self.writer)
            .with_deploy_enabled(false)
            .set_project(project)
            .build()
            .wrap_err("Failed to build pipeline")?
            .run(&self.command.functions)
            .await?;

        Ok(())
    }
}
