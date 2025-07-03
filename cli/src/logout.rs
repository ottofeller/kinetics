use crate::client::Client;
use crate::config::build_config;
use crate::credentials::Credentials;
use serde_json::json;
use std::path::Path;

async fn remove(email: &str) -> eyre::Result<()> {
    let client = Client::new(false)?;

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

pub async fn logout() -> eyre::Result<()> {
    let path = Path::new(&build_config()?.credentials_path);
    let credentials = Credentials::new(path)?;

    if credentials.is_valid(&credentials.email) {
        remove(&credentials.email).await?;
    }

    if path.exists() {
        std::fs::remove_file(path)?;
    }

    Ok(())
}
