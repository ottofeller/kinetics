use crate::api::request::Validate;
use crate::error::Error;
use crate::{api::auth, client::Client};
use chrono::{DateTime, Local};
use eyre::{Context, Result};

/// Creates a new authentication token
pub async fn create(name: &str, period: &Option<String>) -> Result<()> {
    println!(
        "\n{}...",
        console::style("Requesting new access token").bold().green()
    );

    let client = Client::new(false).await?;

    let request = auth::tokens::create::Request {
        name: name.into(),
        period: period.to_owned(),
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
        .json::<auth::tokens::create::Response>()
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

/// Deletes an access token
pub async fn delete(name: &str) -> Result<()> {
    println!(
        "\n{}...",
        console::style("Deleting access token").bold().green()
    );

    let client = Client::new(false).await?;
    let request = auth::tokens::delete::Request { name: name.into() };

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
