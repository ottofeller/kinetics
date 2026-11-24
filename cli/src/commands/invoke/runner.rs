use crate::build;
use crate::commands::invoke::InvokeCommand;
use crate::error::Error;
use crate::function::{Function, Params};
use crate::runner::Runner;
use crate::writer::Writer;
use eyre::WrapErr;
use http::Method;
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
        let all_functions = build::generate_workspace(
            &project,
            std::slice::from_ref(&self.command.name),
            None,
            &project.build_path()?,
        )?;

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

            self.local(&function, &all_functions, migrations_path.as_deref())
                .await?;
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
            Role::Worker | Role::DlqWorker => {
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

    /// Resolves and validates the HTTP method for the function invocation.
    ///
    /// Defaults to POST, falling back to the first method declared
    /// on the endpoint when it does not accept POST.
    pub(super) fn resolve_method(&self, function: &Function) -> eyre::Result<Method> {
        // The method only shapes endpoint requests, any other role ignores it.
        let declared = match &function.params {
            Params::Endpoint(endpoint) => endpoint
                .methods
                .iter()
                .filter_map(|method| Method::from_bytes(method.as_bytes()).ok())
                .collect::<Vec<Method>>(),
            _ => return Ok(self.command.method.clone().unwrap_or(Method::POST)),
        };

        let Some(requested) = &self.command.method else {
            // The invocation carries a payload, so prefer POST whenever the endpoint takes it.
            return Ok(if declared.is_empty() || declared.contains(&Method::POST) {
                Method::POST
            } else {
                declared[0].clone()
            });
        };

        if !declared.is_empty() && !declared.contains(requested) {
            let available = declared
                .iter()
                .map(|method| method.as_str())
                .collect::<Vec<&str>>()
                .join(", ");

            eyre::bail!(
                "Endpoint {} does not accept method {}. Available: [{}]",
                function.name,
                requested,
                available
            );
        }

        Ok(requested.clone())
    }
}
