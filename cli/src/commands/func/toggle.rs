use crate::api::func;
use crate::error::Error;
use crate::function::Function;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;
use http::StatusCode;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct StopCommand {
    /// Function name to stop
    #[arg()]
    name: String,
}

impl Runnable for StopCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        ToggleRunner {
            name: self.name.clone(),
            op: func::toggle::Op::Stop,
            writer,
        }
    }
}

#[derive(clap::Args, Clone)]
pub(crate) struct StartCommand {
    /// Function name to start
    #[arg()]
    name: String,
}

impl Runnable for StartCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        ToggleRunner {
            name: self.name.clone(),
            op: func::toggle::Op::Start,
            writer,
        }
    }
}

struct ToggleRunner<'a> {
    name: String,
    op: func::toggle::Op,
    writer: &'a Writer,
}

impl Runner for ToggleRunner<'_> {
    /// Adds/removes throttling from a function.
    ///
    /// - For start operation the function starts receiving requests.
    /// - For stop operation the function stops receiving requests
    ///   and the endpoint starts responding "Service Unavailable".
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        // Get all function names without any additional manipulations.
        let all_functions = project
            .functions()
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        let function = Function::find_by_name(&all_functions, &self.name).map_err(|e| {
            self.error(
                Some("Could not find requested function"),
                None,
                Some(e.into()),
            )
        })?;

        let client = self.api_client().await?;

        self.writer.text(&format!(
            "\n{} {}...\n\n",
            console::style(format!("{}", self.op)).bold().green(),
            console::style(&function.name).bold()
        ))?;

        let response = client
            .post("/function/toggle")
            .json(&func::toggle::Request {
                project_name: project.name.clone(),
                function_name: function.name,
                operation: self.op.clone(),
            })
            .send()
            .await
            .wrap_err(format!("Failed to send {:?} request", self.op))
            .map_err(|e| self.server_error(Some(e.into())))?;

        let is_throttled = match self.op {
            func::toggle::Op::Start => false,
            func::toggle::Op::Stop => true,
        };

        match response.status() {
            status if status.is_success() => {
                self.writer
                    .text(&format!("{}\n", console::style("Done").bold().green()))?;

                self.writer
                    .json(json!({"success": true, "is_throttled": is_throttled}))?;

                Ok(())
            }
            StatusCode::NOT_MODIFIED => {
                let message = format!(
                    "Nothing changed. Function is {} throttled.",
                    match self.op {
                        func::toggle::Op::Start => "not",
                        func::toggle::Op::Stop => "already",
                    }
                );

                self.writer
                    .text(&format!("{}\n", console::style(&message).yellow()))?;

                self.writer.json(
                    json!({"success": true, "message": message, "is_throttled": is_throttled}),
                )?;

                Ok(())
            }
            StatusCode::FORBIDDEN => {
                let func::toggle::Response { reason, .. } = response
                    .json()
                    .await
                    .wrap_err("Invalid response from server")
                    .map_err(|e| self.server_error(Some(e.into())))?;

                let message = format!("Function is stopped by platform. {reason}");

                self.writer
                    .text(&format!("{}\n", console::style(&message).yellow()))?;

                self.writer
                    .json(json!({"success": false, "message": message}))?;

                Ok(())
            }
            err_status => {
                log::error!(
                    "Failed to call {:?} from API ({err_status}): {}",
                    self.op,
                    response.text().await.unwrap_or("Unknown error".to_string()),
                );

                Err(self.server_error(None))
            }
        }
    }
}
