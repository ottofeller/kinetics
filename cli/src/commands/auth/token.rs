use crate::api::request::Validate;
use crate::error::Error;
use crate::{api::auth, client::Client};
use crossterm::style::Stylize;
use eyre::{Context, Result};

/// Creates a new authentication token
pub async fn create(name: &str, period: &Option<String>) -> Result<()> {
    let client = Client::new(false).await?;
    println!("\n{}...", "Requesting new access token".bold().green());

    let request = auth::token::create::Request {
        name: name.into(),
        period: period.to_owned(),
    };

    let errors = request.validate();

    if errors.is_some() {
        return Err(Error::new("Validation failed", Some(&errors.unwrap().join("\n"))).into());
    }

    let response = client
        .post("/auth/token/create")
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
        .json::<auth::token::create::Response>()
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
