use crate::api::auth::tokens::create::{Request, Response};
use crate::api::request::Validate;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context;

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
    fn runner(&self, _writer: &Writer) -> impl Runner {
        CreateRunner {
            command: self.clone(),
        }
    }
}

struct CreateRunner {
    command: CreateCommand,
}

impl Runner for CreateRunner {
    /// Creates a new authentication token
    async fn run(&mut self) -> Result<(), Error> {
        println!(
            "\n{}...",
            console::style("Requesting new access token").bold().green()
        );

        let client = self.api_client().await?;

        let request = Request {
            name: self.command.name.clone(),
            period: self.command.period.clone(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))).into());
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

            return Err(Error::new("Failed to create token", Some("Try again later.")).into());
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

        println!("{}", console::style(&token).dim());
        Ok(())
    }
}
