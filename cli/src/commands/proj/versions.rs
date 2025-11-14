use crate::client::Client;
use crate::error::Error;
use crate::project::Project;
use chrono::{DateTime, Utc};
use color_eyre::owo_colors::OwoColorize;
use crossterm::style::Stylize;
use eyre::Context;
use serde::{Deserialize, Serialize};

/// Prints out the list of all available versions for the project
pub async fn versions(project: &Project) -> Result<(), Error> {
    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct Version {
        version: u32,
        updated_at: DateTime<Utc>,
        message: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Response {
        versions: Vec<Version>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Body {
        name: String,
    }

    let client = Client::new(false)
        .await
        .inspect_err(|e| log::error!("Failed to create client: {e:?}"))
        .wrap_err("Authentication failed. Please login first.")?;

    let mut versions = client
        .request::<Body, Response>(
            "/stack/versions",
            Body {
                name: project.name.clone(),
            },
        )
        .await
        .inspect_err(|e| log::error!("Failed to fetch versions: {e:?}"))
        .wrap_err("Failed to fetch project versions. Try again later.")?
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
            v.message
                .unwrap_or("No message".black().dimmed().to_string())
        );
    }

    Ok(())
}
