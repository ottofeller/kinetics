use crate::commands::build::prepare_functions;
use crate::commands::invoke::remote;
use crate::commands::invoke::InvokeCommand;
use crate::config::build_config;
use crate::error::Error;
use crate::function::Function;
use crate::runner::Runner;
use std::path::PathBuf;

pub(crate) struct InvokeRunner {
    pub(crate) command: InvokeCommand,
}

impl Runner for InvokeRunner {
    /// Invoke the function either locally or remotely
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        // Get function names as well as pull all updates from the code.
        let all_functions = prepare_functions(
            PathBuf::from(build_config()?.kinetics_path),
            &project,
            &[self.command.name.clone().into()],
        )?;

        let function = Function::find_by_name(&all_functions, &self.command.name)?;

        // If --with_migrations was not passed, or comes with default "" value, then
        // do not set the migrations path. There is a default value set down the flow.
        let migrations_path = if self
            .command
            .with_migrations
            .clone()
            .unwrap_or_default()
            .is_empty()
        {
            None
        } else {
            self.command.with_migrations.clone()
        };

        if !self.command.remote {
            self.local(&function, migrations_path.as_deref()).await?
        } else {
            remote::invoke(
                &function,
                &project,
                self.command.payload.as_deref(),
                self.command.headers.as_deref(),
                self.command.url_path.as_deref(),
            )
            .await?
        }

        Ok(())
    }
}
