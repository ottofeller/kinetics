use std::io::{stdin, stdout, Write};
use crate::api::auth::tokens::delete::Request;
use crate::api::request::Validate;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use crossterm::style::Stylize;
use eyre::Context;

#[derive(clap::Args, Clone)]
pub(crate) struct DeleteCommand {
    /// Name of the access token to delete
    name: String,
}

impl Runnable for DeleteCommand {
    fn runner(&self, _writer: &Writer) -> impl Runner {
        DeleteRunner {
            command: self.clone(),
        }
    }
}

struct DeleteRunner {
    command: DeleteCommand,
}

impl Runner for DeleteRunner {
    /// Deletes an access token
    async fn run(&mut self) -> Result<(), Error> {
        let generic_error = Error::new(
            "Failed to process the command",
            Some("Please report a bug at support@deploykinetics.com"),
        );

        // Ask for confirmation
        print!(
            "\nDelete access token {}? {} ",
            self.command.name.clone().bold(),
            "[y/N]".dim()
        );

        let mut input = String::new();

        stdout().flush().map_err(|e| {
            log::error!("Failed to write to stdout: {e:?}");
            generic_error.clone()
        })?;

        stdin().read_line(&mut input).map_err(|e| {
            log::error!("Failed to read from stdin: {e:?}");
            generic_error
        })?;

        if !matches!(input.trim().to_lowercase().as_ref(), "y" | "yes") {
            println!("{}", "Canceled".yellow());
            return std::result::Result::Ok(());
        }

        println!(
            "\n{}...",
            console::style("Deleting access token").bold().green()
        );

        let client = self.api_client().await?;

        let request = Request {
            name: self.command.name.clone(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))).into());
        }

        let response = client
            .post("/auth/tokens/delete")
            .json(&request)
            .send()
            .await
            .wrap_err("Failed to call token deletion endpoint")?;

        if !response.status().is_success() {
            let response: serde_json::Value = response.json().await.wrap_err(Error::new(
                "Invalid response from server",
                Some("Try again later."),
            ))?;

            return Err(Error::new(
                "Failed to delete token",
                Some(
                    response
                        .get("error")
                        .unwrap_or(&serde_json::Value::Null)
                        .as_str()
                        .unwrap_or("Unknown error"),
                ),
            )
            .into());
        }

        println!("\n{}", console::style("Deleted").green().bold());
        Ok(())
    }
}
