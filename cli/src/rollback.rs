use crate::client::Client;
use crate::crat::Crate;
use crate::error::Error;
use eyre::{Context, Result};
use serde_json::json;

/// Rollback a project by one version
///
/// Consequent rollbacks are possible and will revert one version at a time
pub async fn rollback(crat: &Crate) -> Result<()> {
    let client = Client::new(false).wrap_err("Failed to create client")?;
    println!(
        "{}...\n{}",
        console::style("Rolling back").bold().green(),
        console::style("Reverting to the previous version...").dim()
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
