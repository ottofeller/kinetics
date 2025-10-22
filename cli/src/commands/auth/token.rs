use crate::client::Client;
use crate::error::Error;
use crossterm::style::Stylize;
use eyre::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TokenResponse {
    token: String,
}

/// Creates a new authentication token
pub async fn token(period: &Option<String>) -> Result<()> {
    let client = Client::new(false).await?;
    println!("\n{}...", "Requesting new access token".bold().green());

    let response = client
        .post("/auth/token/create")
        .json(&serde_json::json!({"period": period}))
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
        .json::<TokenResponse>()
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
