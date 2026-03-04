use crate::api::client::Client;
use crate::api::stack;
use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use color_eyre::owo_colors::OwoColorize;
use crossterm::style::Stylize;
use eyre::Context;
use serde_json::{json, Value};

#[derive(clap::Args, Clone)]
pub(crate) struct VersionsCommand {}

impl Runnable for VersionsCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        VersionsRunner { writer }
    }
}

struct VersionsRunner<'a> {
    writer: &'a Writer,
}

impl Runner for VersionsRunner<'_> {
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
            self.writer
                .text(&format!("{}", "No versions found".yellow()))?;

            self.writer.json(json!({"success": true, "versions": []}))?;
            return Ok(());
        }

        // Show the latest version at the bottom
        versions.reverse();

        let mut versions_json: Vec<Value> = vec![];

        for v in &versions {
            let updated_at = v.updated_at.format("%Y-%m-%d %H:%M:%S").to_string();

            self.writer.text(&format!(
                "{} {}\n{}\n\n",
                v.version.to_string().bold(),
                updated_at.dimmed(),
                "No message".black().dimmed()
            ))?;

            versions_json.push(json!({
                "version": v.version,
                "updated_at": v.updated_at,
            }));
        }

        self.writer
            .json(json!({"success": true, "versions": versions_json}))?;

        Ok(())
    }
}
