use crate::api::error::ApiError;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use chrono::{DateTime, Local};
use eyre::Context;
use kinetics_api::auth::tokens::list::Response;
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
            "\n{} access tokens...\n\n",
            console::style("Fetching").bold()
        ))?;

        let response = client
            .post("/auth/tokens/list")
            .send()
            .await
            .wrap_err("Failed to call token list endpoint")?;

        if !response.status().is_success() {
            return Err(ApiError::from_response(response).await?.into());
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
                console::style(&token.name),
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
