use crate::commands::login::LoginCommand;
use crate::credentials::Credentials;
use crate::error::Error;
use crate::project::Project;
use crate::runner::Runner;
use crate::{api::auth, config::api_url};
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use eyre::Context;
use regex::Regex;
use std::io::{self, Write};
pub(crate) struct LoginRunner {
    pub(crate) command: LoginCommand,
}

impl Runner for LoginRunner {
    /// Obtains the access token
    ///
    /// Returns boolean, indicating whether the new login session was
    /// created or not (the old one not expired).
    ///
    /// The procedure is rather simple and should be improved as the CLI develops. It sends a one-time code to email
    /// and after user enters it in stdin exchanges it for short lived access token.
    async fn run(&mut self) -> Result<(), Error> {
        // Validate email
        if !Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")
            .map_err(|e| {
                log::error!("Regex parsing failed: {e:?}");
                Error::new(
                    "Failed to parse email",
                    Some("Please report a bug at support@deploykinetics.com"),
                )
            })?
            .is_match(&self.command.email)
        {
            return Err(Error::new("Invalid email format", None));
        }

        let mut credentials = Credentials::new().await?;
        let mut is_new_session = false;

        // If credentials expired — request new token
        if !credentials.is_valid() || credentials.email != self.command.email {
            credentials.write(self.request().await?)?;
            is_new_session = true;
        }

        println!(
            "{} {} {}",
            console::style(if is_new_session {
                "Successfully logged in"
            } else {
                "Already logged in"
            })
            .green()
            .bold(),
            console::style("via").dim(),
            console::style(&self.command.email).underlined().bold()
        );

        Ok(())
    }
}

impl LoginRunner {
    /// Request auth code and exchange it for access token
    async fn request(&self) -> eyre::Result<Credentials> {
        // Refresh the token if it is expired
        let client = reqwest::Client::new();

        let response = client
            .post(api_url("/auth/code/request"))
            .json(&auth::code::request::Request {
                email: self.command.email.clone(),
            })
            .send()
            .await
            .wrap_err(Error::new(
                "Network request failed",
                Some("Try again in a few seconds."),
            ))?;

        if !response.status().is_success() {
            log::error!("Got error response: {}", response.text().await?);

            return Err(Error::new(
                "Failed to request auth code",
                Some("Try again in a few seconds."),
            )
            .into());
        }

        println!("Please enter the one-time code sent to your email:");
        let code = self.read_masked_password()?;
        let code = code.trim();

        let response = client
            .post(api_url("/auth/code/exchange"))
            .json(&auth::code::exchange::Request {
                email: self.command.email.clone(),
                code: code.to_owned(),
            })
            .send()
            .await
            .wrap_err(Error::new(
                "Network request failed",
                Some("Try again in a few seconds."),
            ))?;

        if response.status().is_client_error() {
            return Err(Error::new(
                "Failed to log in",
                Some("The one-time code has expired or is invalid."),
            )
            .into());
        }

        if !response.status().is_success() {
            return Err(Error::new("Failed to log in", Some("Try again in a few seconds.")).into());
        }

        // Projects cache is currently holding only one user projects. Clear it to avoid
        // overlapping settings.
        Project::clear_cache()?;

        Ok(response.json().await?)
    }

    fn read_masked_password(&self) -> eyre::Result<String> {
        let mut password = String::new();
        enable_raw_mode().wrap_err(Error::new("Failed to change terminal mode", None))?;

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
                                .wrap_err(Error::new("Could not modify stdout", None))?;
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
                            .wrap_err(Error::new("Could not modify stdout", None))?;
                    }
                    _ => {}
                }
            }
        }

        disable_raw_mode()?;
        println!();
        Ok(password)
    }
}
