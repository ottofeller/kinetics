use crate::api::client::Client;
use crate::api::stack;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use color_eyre::owo_colors::OwoColorize;
use crossterm::style::Stylize;
use eyre::Context;

#[derive(clap::Args, Clone)]
pub(crate) struct VersionsCommand {}

impl Runnable for VersionsCommand {
    fn runner(&self) -> impl Runner {
        VersionsRunner {
            command: self.clone(),
        }
    }
}

struct VersionsRunner {
    command: VersionsCommand,
}

impl Runner for VersionsRunner {
    /// Prints out the list of all available versions for the project
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        let client = Client::new(false)
            .await
            .inspect_err(|e| log::error!("Failed to create client: {e:?}"))
            .wrap_err("Authentication failed. Please login first.")
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        let mut versions = client
            .request::<_, stack::versions::Response>(
                "/stack/versions",
                stack::versions::Request {
                    name: project.name.clone(),
                },
            )
            .await
            .inspect_err(|e| log::error!("Failed to fetch versions: {e:?}"))
            .wrap_err("Failed to fetch project versions. Try again later.")
            .map_err(|e| self.error(None, None, Some(e.into())))?
            .versions;

        if versions.is_empty() {
            println!("{}", "No versions found".yellow());
            return Ok(());
        }

        // Show the latest version at the bottom
        versions.reverse();

        for v in versions {
            println!(
                "{} {}\n{}\n",
                v.version.to_string().bold(),
                v.updated_at
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
                    .dimmed(),
                "No message".black().dimmed()
            );
        }

        Ok(())
    }
}
