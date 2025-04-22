use crate::config::build_config;
use crate::config::{api_url, Credentials};
use crate::error::Error;
use chrono::Utc;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use eyre::Context;
use regex::Regex;
use serde_json::json;
use std::io::{self, Write};
use std::path::Path;

/// Request auth code and exchange it for access token
async fn request(email: &str) -> eyre::Result<Credentials> {
    // Refresh the token if it is expired
    let client = reqwest::Client::new();

    let response = client
        .post(api_url("/auth/code/request"))
        .json(&json!({ "email": email }))
        .send()
        .await
        .wrap_err(Error::new(
            "Network request failed",
            Some("Try again in a few seconds."),
        ))?;

    if !response.status().is_success() {
        return Err(Error::new(
            "Failed to request auth code",
            Some("Try again in a few seconds."),
        )
        .into());
    }

    println!("Please enter the one-time code sent to your email:");

    let code = read_masked_password()?;
    let code = code.trim();

    let response = client
        .post(api_url("/auth/code/exchange"))
        .json(&json!({ "email": email, "code": code }))
        .send()
        .await
        .wrap_err(Error::new(
            "Network request failed",
            Some("Try again in a few seconds."),
        ))?;

    if response.status().is_client_error() {
        return Err(eyre::eyre!("The email or one-time code are invalid"));
    }

    if !response.status().is_success() {
        return Err(Error::new("Failed to log in", Some("Try again in a few seconds.")).into());
    }

    Ok(response.json::<Credentials>().await?)
}

/// Obtain the access token
///
/// Returns boolean, indicating whether the new login session was
/// created or not (the old one not expired).
///
/// The procedure is rather simple and should be improved as the CLI develops. It sends a one-time code to email
/// and after user enters it in stdin exhcbages it for short lived access token.
pub async fn login(email: &str) -> eyre::Result<bool> {
    // Validate email
    if !Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")?.is_match(email) {
        return Err(eyre::eyre!("Invalid email format"));
    }

    let path = Path::new(&build_config()?.build_path).join(".credentials");

    let default =
        json!({ "email": "", "token": "", "expiresAt": "2000-01-01T00:00:00Z" }).to_string();

    // Read or create credentials file
    let credentials = serde_json::from_str::<Credentials>(
        &std::fs::read_to_string(path.clone())
            .or_else(|_| {
                std::fs::write(path.clone(), default.clone()).wrap_err(Error::new(
                    "Failed to write credentials file",
                    Some(
                        "File system issue, check the file permissions in ~/.kinetics/.credentials",
                    ),
                ))?;

                eyre::Ok(default.clone())
            })
            .unwrap_or(default),
    )
    .wrap_err(Error::new(
        "Could not parse credentials file",
        Some("Delete ~/.kinetics/.credentials and try again"),
    ))?;

    // If credentials expired — request new token
    if !credentials.token.is_empty()
        && credentials.expires_at.timestamp() > Utc::now().timestamp()
        && credentials.email == email
    {
        return Ok(false);
    }

    std::fs::write(path, json!(request(email).await?).to_string()).wrap_err(Error::new(
        "Failed to store credentials",
        Some("File system issue, check the file permissions in ~/.kinetics/.credentials"),
    ))?;

    Ok(true)
}

fn read_masked_password() -> eyre::Result<String> {
    let mut password = String::new();
    enable_raw_mode().wrap_err(Error::new("Failed to chaneg terminal mode", None))?;

    loop {
        // Read a key event
        if let Event::Key(key_event) =
            event::read().wrap_err(Error::new("Failed to read keyboard event", None))?
        {
            match key_event.code {
                KeyCode::Enter => {
                    break;
                }
                KeyCode::Backspace => {
                    if !password.is_empty() {
                        password.pop();
                        // Erase the last asterisk (use actual backspace character)
                        print!("\x08 \x08");
                        io::stdout()
                            .flush()
                            .wrap_err(Error::new("Could not modofy stdout", None))?;
                    }
                }
                // Handle Ctrl+C to exit
                KeyCode::Char('c') if key_event.modifiers == event::KeyModifiers::CONTROL => {
                    disable_raw_mode()?;
                    return Err(eyre::eyre!("Password input cancelled by user"));
                }
                KeyCode::Char(c) => {
                    password.push(c);
                    print!("*");
                    io::stdout()
                        .flush()
                        .wrap_err(Error::new("Could not modofy stdout", None))?;
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    println!();

    Ok(password)
}
