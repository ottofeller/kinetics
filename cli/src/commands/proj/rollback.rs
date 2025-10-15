use crate::crat::Crate;
use crate::{client::Client, error::Error};
use chrono::{DateTime, Utc};
use eyre::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VersionsRequest {
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RollbackRequest {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VersionsResponse {
    versions: Vec<Version>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Version {
    version: u32,
    updated_at: DateTime<Utc>,
}

/// Rollback a project by one version or to a specific version
///
/// Consequent rollbacks are possible and will revert one version at a time
/// If version is specified, rollback to that specific version
pub async fn rollback(crat: &Crate, version: Option<u32>) -> Result<()> {
    let client = Client::new(false).wrap_err("Failed to create client")?;

    let versions: VersionsResponse = client
        .request(
            "/stack/versions",
            VersionsRequest {
                name: crat.name.to_string(),
            },
        )
        .await?;

    if versions.versions.len() < 2 && version.is_none() {
        println!(
            "{}",
            console::style("Nothing to rollback, there is only one version").yellow()
        );
        return Ok(());
    }

    let target_version = match version {
        Some(v) => {
            // Find the specified version in the list
            if let Some(target) = versions.versions.iter().find(|ver| ver.version == v) {
                target.clone()
            } else {
                return Err(Error::new(
                    &format!("Version {} not found", v),
                    Some("Use 'kinetics proj versions' to see available versions."),
                )
                .into());
            }
        }
        None => {
            // Default behavior: rollback to previous version
            versions.versions[1].clone()
        }
    };

    println!(
        "{} {} {}...",
        console::style("Rolling back").bold().green(),
        console::style("to").dim(),
        console::style(format!(
            "v{} ({})",
            target_version.version,
            target_version.updated_at.with_timezone(&chrono::Local)
        ))
        .bold(),
    );

    client
        .post("/stack/rollback")
        .json(&RollbackRequest {
            name: crat.name.to_string(),
            version,
        })
        .send()
        .await?;

    let mut status = crat.status().await?;

    // Poll the status of the rollback
    while status.status == "IN_PROGRESS" {
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        status = crat.status().await?;
    }

    if status.status == "FAILED" {
        return Err(Error::new("Rollback failed", Some("Try again in a few seconds.")).into());
    }

    println!("{}", console::style("Done").green());
    Ok(())
}
