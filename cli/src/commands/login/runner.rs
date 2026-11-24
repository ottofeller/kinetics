use crate::api::client::Client;
use crate::api::error::ApiError;
use crate::commands::login::LoginCommand;
use crate::credentials::Credentials;
use crate::error::Error;
use crate::project::Project;
use crate::runner::Runner;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use eyre::Context;
use kinetics_api::auth;
use kinetics_api::request::Validate;
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
        let request = auth::code::request::Request {
            email: self.command.email.clone(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))));
        }

        let mut credentials = Credentials::new().await?;
        let mut is_new_session = false;

        // If credentials expired — request new token
        if !credentials.is_valid() || credentials.email != self.command.email {
            credentials.write(self.request(request).await?)?;
            is_new_session = true;
        }

        println!(
            "{} logged in via {}",
            console::style(if is_new_session {
                "Successfully"
            } else {
                "Already"
            })
            .bold(),
            console::style(&self.command.email).underlined()
        );

        Ok(())
    }
}

impl LoginRunner {
    /// Request auth code and exchange it for access token
    async fn request(&self, request: auth::code::request::Request) -> Result<Credentials, Error> {
        // Refresh the token if it is expired
        let client = Client::unauthenticated();

        let response = client
            .post("/auth/code/request")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ApiError::from_response(response).await?.into());
        }

        println!("Please enter the one-time code sent to your email:");
        let code = self.read_masked_password()?;
        let code = code.trim();

        let request = auth::code::exchange::Request {
            email: self.command.email.clone(),
            code: code.to_owned(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))));
        }

        let response = client
            .post("/auth/code/exchange")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ApiError::from_response(response).await?.into());
        }

        // Projects cache is currently holding only one user projects. Clear it to avoid
        // overlapping settings.
        Project::clear_cache()?;

        Ok(response
            .json()
            .await
            .wrap_err("Could not parse login response")?)
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
                    KeyCode::Backspace if !password.is_empty() => {
                        password.pop();
                        // Erase the last asterisk (use actual backspace character)
                        print!("\x08 \x08");
                        io::stdout()
                            .flush()
                            .wrap_err(Error::new("Could not modify stdout", None))?;
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
