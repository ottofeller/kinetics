use crate::{client::Client, error::Error};
use crate::crat::Crate;
use chrono::{DateTime, Utc};
use eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Body {
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Response {
    versions: Vec<Version>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Version {
    version: u32,
    update_at: DateTime<Utc>,
}

/// Rollback a project by one version
///
/// Consequent rollbacks are possible and will revert one version at a time
pub async fn rollback(crat: &Crate) -> Result<()> {
    let client = Client::new(false).wrap_err("Failed to create client")?;

    let versions: Response = client
        .request(
            "/stack/versions",
            Body {
                name: crat.name.to_string(),
            },
        )
        .await?;

    if versions.versions.len() < 2 {
        println!(
            "{}",
            console::style("Nothing to rollback, there is only one version").yellow()
        );
        return Ok(());
    }

    println!(
        "{} {} {}...",
        console::style("Rolling back").bold().green(),
        console::style("to").dim(),
        console::style(format!(
            "v{} ({})",
            versions.versions[1].version,
            versions.versions[1].update_at.with_timezone(&chrono::Local)
        ))
        .bold(),
    );

    client
        .post("/stack/rollback")
        .json(&json!({"name": crat.name}))
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
