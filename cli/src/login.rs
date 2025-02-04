use chrono::{DateTime, Utc};
use eyre::Context;
use regex::Regex;
use serde_json::json;
use std::{io, path::Path};

#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    token: String,
    expires_at: DateTime<Utc>,
}

/// Return the cached access token, or refresh if it is expired
async fn token(email: &str) -> eyre::Result<String> {
    let path = Path::new(&crate::skypath()?).join(".credentials");

    // Read or create credentials file
    let credentials = serde_json::from_str::<Credentials>(
        &std::fs::read_to_string(path.clone())
            .or_else(|_| {
                let default =
                    json!({ "token": "", "expiresAt": "2000-01-01T00:00:00Z" }).to_string();

                std::fs::write(path.clone(), default.clone())?;
                eyre::Ok(default.into())
            })
            .unwrap_or_default(),
    )
    .wrap_err("Credentials stored in a wrong format")?;

    if !credentials.token.is_empty() && credentials.expires_at.timestamp() > Utc::now().timestamp()
    {
        return Ok(credentials.token);
    }

    // Refresh the token if it is expired
    let client = reqwest::Client::new();

    let response = client
        .post(&crate::api_url("/auth/code/request"))
        .json(&serde_json::json!({ "email": email }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(eyre::eyre!("Failed to request the auth code"));
    }

    println!("Enter the code sent to your email: ");
    let mut code = String::new();
    io::stdin().read_line(&mut code)?;
    let code = code.trim();

    let response = client
        .post(&crate::api_url("/auth/code/exchange"))
        .json(&serde_json::json!({ "email": email, "code": code }))
        .send()
        .await?;

    if response.status().is_client_error() {
        return Err(eyre::eyre!("The email or one-time code are invalid"));
    }

    if !response.status().is_success() {
        return Err(eyre::eyre!("Failed to login: {}", response.text().await?));
    }

    let response: Credentials = response.json::<Credentials>().await?;
    std::fs::write(path, json!(response).to_string())?;
    Ok(response.token)
}

/// Obtain the access token
///
/// The procedure is rather simple and should be improved as the CLI develops. It sends a one-time code to email
/// and after user enters it in stdin exhcbages it for short lived access token.
pub async fn login(email: &str) -> eyre::Result<()> {
    // Validate email
    if !Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")?.is_match(&email) {
        return Err(eyre::eyre!("Invalid email format"));
    }

    token(email).await?;
    Ok(())
}
