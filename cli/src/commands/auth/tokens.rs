mod create;
mod delete;
use crate::api::{auth, client::Client};
use crate::commands::auth::tokens::create::CreateCommand;
use crate::commands::auth::tokens::delete::DeleteCommand;
use crate::error::Error;
use chrono::{DateTime, Local};
use clap::Subcommand;
use eyre::{Context, Result};

/// Fetch and list all access tokens
pub async fn list() -> Result<()> {
    let client = Client::new(false).await?;

    println!(
        "\n{}...\n",
        console::style("Fetching access tokens").bold().green()
    );

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
        .json::<auth::tokens::list::Response>()
        .await
        .inspect_err(|e| log::error!("Failed to parse tokens response: {}", e))
        .wrap_err(Error::new(
            "Invalid response from server",
            Some("Try again later."),
        ))?
        .0;

    if tokens.is_empty() {
        println!("{}", console::style("No tokens found").yellow());
        return Ok(());
    }

    for token in tokens {
        let expires_at_local: DateTime<Local> = token.expires_at.into();

        println!(
            "{}\n{}\n",
            console::style(&token.name).bold(),
            console::style(format!(
                "Expires at {}",
                expires_at_local.format("%d %b %Y %H:%M:%S")
            ))
            .dim(),
        );
    }

    Ok(())
}

#[derive(Subcommand)]
pub(crate) enum TokensCommands {
    /// Create a new access token
    Create(CreateCommand),

    /// Delete an access token
    Delete(DeleteCommand),
}
