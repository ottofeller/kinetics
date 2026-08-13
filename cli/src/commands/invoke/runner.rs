use crate::commands::invoke::InvokeCommand;
use crate::error::Error;
use crate::function::Function;
use crate::runner::Runner;
use crate::writer::Writer;
use eyre::WrapErr;
use kinetics_parser::Role;
use serde_json::Value;
use std::fs;

pub(crate) struct InvokeRunner<'a> {
    pub(crate) command: InvokeCommand,
    pub(crate) writer: &'a Writer,
}

impl Runner for InvokeRunner<'_> {
    /// Invoke the function either locally or remotely
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project(&self.command.project).await?;

        // Get function names as well as pull all updates from the code.
        let all_functions = project.parse(std::slice::from_ref(&self.command.name))?;

        let function = Function::find_by_name(&all_functions, &self.command.name)?;

        if !self.command.remote {
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

            self.local(&function, migrations_path.as_deref()).await?
        } else {
            self.remote(function).await?
        }

        Ok(())
    }
}

impl InvokeRunner<'_> {
    /// Resolves and validates the invocation payload for the function role.
    pub(super) fn resolve_payload(&self, role: &Role) -> eyre::Result<Option<String>> {
        // Construct payload from either the payload string or the payload file if any is provided
        let payload = match self.command.payload.as_deref() {
            Some(payload) => Some(payload.to_owned()),
            None => self
                .command
                .payload_file
                .as_deref()
                .map(|p| {
                    fs::read_to_string(p)
                        .wrap_err_with(|| format!("Failed to read payload file {}", p.display()))
                })
                .transpose()?,
        };

        // Resolve the final payload based on the role
        let resolved_payload = match role {
            Role::Endpoint => Some(payload.unwrap_or("{}".to_string())),
            Role::Worker => {
                let payload = payload.unwrap_or("[]".to_string());

                serde_json::from_str::<Vec<Value>>(&payload)
                    .wrap_err("Worker payload must be a top-level JSON array")?;

                Some(payload)
            }
            Role::Cron if payload.is_some() => {
                eyre::bail!("Cron functions do not accept a payload");
            }
            Role::Cron => None,
        };

        Ok(resolved_payload)
    }
}
