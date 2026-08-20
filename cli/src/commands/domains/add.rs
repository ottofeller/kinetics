use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;
use kinetics_api::domains::create::{Request, Response};
use kinetics_api::request::Validate;
use serde_json::json;
use std::path::PathBuf;

#[derive(clap::Args, Clone)]
pub(crate) struct AddCommand {
    /// Domain name to create (e.g. example.com)
    #[arg(value_name = "DOMAIN")]
    domain_name: String,

    /// Relative path to the project directory
    #[arg(long)]
    project: Option<PathBuf>,
}

impl Runnable for AddCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        AddRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct AddRunner<'a> {
    command: AddCommand,
    writer: &'a Writer,
}

impl Runner for AddRunner<'_> {
    /// Attaches a custom domain to the project
    /// Prints domain status and the nameservers to set at the registrar
    async fn run(&mut self) -> Result<(), Error> {
        let mut project = self.project(&self.command.project).await?;
        let client = self.api_client().await?;

        let request = Request {
            project: (&project).into(),
            domain_name: self.command.domain_name.clone(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))));
        }

        self.writer.text(&format!(
            "\n{} domain {}...\n\n",
            console::style("Adding").bold(),
            console::style(&self.command.domain_name).underlined(),
        ))?;

        let response = client
            .post("/domains/create")
            .json(&request)
            .send()
            .await
            .wrap_err("Failed to call /domains/create endpoint")
            .map_err(|e| self.server_error(Some(e.into())))?;

        if !response.status().is_success() {
            log::error!(
                "Failed to add domain ({}): {}",
                response.status(),
                response.text().await.unwrap_or("Unknown error".to_string()),
            );

            return Err(self.server_error(None));
        }

        let resp: Response = response
            .json()
            .await
            .inspect_err(|e| log::error!("Failed to parse response: {e}"))
            .wrap_err("Invalid response from server")
            .map_err(|e| self.server_error(Some(e.into())))?;

        self.writer.text(&format!(
            "{} {}\n",
            console::style("Status:").dim(),
            console::style(resp.status.to_string()).bold(),
        ))?;

        self.writer.text(&format!(
            "{}\n",
            console::style("Add these nameservers at your domain registrar:").dim(),
        ))?;

        for ns in &resp.nameservers {
            self.writer
                .text(&format!("  {}\n", console::style(ns).underlined()))?;
        }

        project.domain_name = Some(self.command.domain_name.clone());
        project
            .write_config()
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        self.writer.json(json!({
            "success": true,
            "status": resp.status.to_string(),
            "nameservers": resp.nameservers,
        }))?;

        Ok(())
    }
}
