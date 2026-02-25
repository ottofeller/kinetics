use crate::api::auth;
use crate::config::build_config;
use crate::credentials::Credentials;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use eyre::Context;
use std::path::Path;

#[derive(clap::Args, Clone)]
pub(crate) struct LogoutCommand {}

impl Runnable for LogoutCommand {
    fn runner(&self) -> impl Runner {
        LogoutRunner {}
    }
}

struct LogoutRunner;

impl Runner for LogoutRunner {
    /// Logs user out
    ///
    /// By cleaning up the local credentials file and voiding credentials on the backend
    async fn run(&mut self) -> Result<(), Error> {
        let path = Path::new(&build_config()?.credentials_path);
        let credentials = Credentials::new().await?;

        if credentials.is_valid() {
            self.remove(&credentials.email)
                .await
                .wrap_err("Logout request failed")?;
        }

        if path.exists() {
            std::fs::remove_file(path).wrap_err("Failed to delete credentials file")?;
        }

        println!(
            "{}",
            console::style("Successfully logged out").green().bold()
        );

        Ok(())
    }
}

impl LogoutRunner {
    async fn remove(&mut self, email: &str) -> eyre::Result<()> {
        let client = self.api_client().await?;

        let response = client
            .post("/auth/logout")
            .json(&auth::logout::Request {
                email: email.to_owned(),
            })
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(eyre::eyre!("Failed to logout: {}", response.text().await?));
        }

        Ok(())
    }
}
