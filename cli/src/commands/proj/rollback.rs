use crate::project::Project;
use crate::{api::stack, client::Client, error::Error};
use eyre::{Context, Result};

/// Rollback a project by one version or to a specific version
///
/// Consequent rollbacks are possible and will revert one version at a time
/// If version is specified, rollback to that specific version
pub async fn rollback(project: &Project, version: Option<u32>) -> Result<()> {
    let client = Client::new(false)
        .await
        .wrap_err("Failed to create client")?;

    let versions: stack::versions::Response = client
        .request(
            "/stack/versions",
            stack::versions::Request {
                name: project.name.to_string(),
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
        .json(&stack::rollback::Request {
            name: project.name.to_string(),
            version,
        })
        .send()
        .await?;

    let mut status = project.status().await?;

    // Poll the status of the rollback
    while status.status == "IN_PROGRESS" {
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        status = project.status().await?;
    }

    if status.status == "FAILED" {
        return Err(Error::new("Rollback failed", Some("Try again in a few seconds.")).into());
    }

    println!("{}", console::style("Done").green());
    Ok(())
}
