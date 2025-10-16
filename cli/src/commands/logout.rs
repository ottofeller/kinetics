use crate::client::Client;
use crate::config::build_config;
use crate::credentials::Credentials;
use eyre::Context;
use serde_json::json;
use std::path::Path;

async fn remove(email: &str) -> eyre::Result<()> {
    let client = Client::new(false).await?;

    let response = client
        .post("/auth/code/logout")
        .json(&json!({ "email": email }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(eyre::eyre!("Failed to logout: {}", response.text().await?));
    }

    Ok(())
}

/// Logs user out
///
/// By cleaning up the local credentials file and voiding credentials on the backend
pub async fn logout() -> eyre::Result<()> {
    let path = Path::new(&build_config()?.credentials_path);
    let credentials = Credentials::new(path).await?;

    if credentials.is_valid(&credentials.email) {
        remove(&credentials.email)
            .await
            .wrap_err("Logout request failed")?;
    }

    if path.exists() {
        std::fs::remove_file(path).wrap_err("Failed to delete credentials file")?;
    }

    println!(
        "{}",
        console::style("Successfully logged out")
            .green()
            .bold()
    );

    Ok(())
}
