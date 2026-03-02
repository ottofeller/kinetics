use crate::api::auth::tokens::list::Response;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use chrono::{DateTime, Local};
use eyre::Context;
use serde_json::{json, Value};

#[derive(clap::Args, Clone)]
pub(crate) struct ListCommand;

impl Runnable for ListCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        ListRunner { writer }
    }
}

struct ListRunner<'a> {
    writer: &'a Writer,
}

impl Runner for ListRunner<'_> {
    /// Fetch and list all access tokens
    async fn run(&mut self) -> Result<(), Error> {
        let client = self.api_client().await?;

        self.writer.text(&format!(
            "\n{}...\n\n",
            console::style("Fetching access tokens").bold().green()
        ))?;

        let response = client
            .post("/auth/tokens/list")
            .send()
            .await
            .wrap_err("Failed to call token list endpoint")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or("Unknown error".to_string());

            log::error!(
                "Failed to fetch tokens from API ({}): {}",
                status,
                error_text
            );

            return Err(Error::new("Failed to fetch tokens", Some("Try again later.")).into());
        }

        let tokens = response
            .json::<Response>()
            .await
            .inspect_err(|e| log::error!("Failed to parse tokens response: {}", e))
            .wrap_err(Error::new(
                "Invalid response from server",
                Some("Try again later."),
            ))?
            .0;

        if tokens.is_empty() {
            self.writer
                .text(&format!("{}", console::style("No tokens found").yellow()))?;
            self.writer.json(json!({"success": true, "tokens": []}))?;
            return Ok(());
        }

        let mut tokens_json: Vec<Value> = vec![];

        for token in tokens {
            let expires_at_local: DateTime<Local> = token.expires_at.into();
            tokens_json.push(json!({"name": token.name, "expires_at": token.expires_at}));

            self.writer.text(&format!(
                "{}\n{}\n\n",
                console::style(&token.name).bold(),
                console::style(format!(
                    "Expires at {}",
                    expires_at_local.format("%d %b %Y %H:%M:%S")
                ))
                .dim(),
            ))?;
        }

        self.writer
            .json(json!({"success": true, "tokens": tokens_json}))?;

        Ok(())
    }
}
