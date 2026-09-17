use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;
use kinetics_api::auth::tokens::create::{Request, Response};
use kinetics_api::request::Validate;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct CreateCommand {
    /// Time period for which the token is active (e.g. `1day`, or `3hours`, or `5d`).
    ///
    /// Defaults to 30days.
    #[arg(short, long)]
    period: Option<String>,

    /// Unique name for the access token, across the project.
    name: String,
}

impl Runnable for CreateCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        CreateRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct CreateRunner<'a> {
    command: CreateCommand,
    writer: &'a Writer,
}

impl Runner for CreateRunner<'_> {
    /// Creates a new authentication token
    async fn run(&mut self) -> Result<(), Error> {
        self.writer.text(&format!(
            "\n{} new access token...\n",
            console::style("Requesting").bold()
        ))?;

        let client = self.api_client().await?;

        let request = Request {
            name: self.command.name.clone(),
            period: self.command.period.clone(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))));
        }

        let response = client
            .post("/auth/tokens/create")
            .json(&request)
            .send()
            .await
            .wrap_err("Failed to call token creation endpoint")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or("Unknown error".to_string());

            log::error!(
                "Failed to create token from API ({}): {}",
                status,
                error_text
            );

            return Err(Error::new(
                "Failed to create token",
                Some("Try again later."),
            ));
        }

        let token = response
            .json::<Response>()
            .await
            .inspect_err(|e| log::error!("Failed to parse token response: {}", e))
            .wrap_err(Error::new(
                "Invalid response from server",
                Some("Try again later."),
            ))?
            .token;

        self.writer
            .text(&format!("{}\n", console::style(&token).dim()))?;

        self.writer.json(json!({"success": true, "token": token}))?;
        Ok(())
    }
}
