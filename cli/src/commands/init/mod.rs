use crate::error::Error;
use crate::function::Type as FunctionType;
use eyre::eyre;
use eyre::WrapErr;
use reqwest::Response;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use toml_edit::value;
use toml_edit::DocumentMut;

const CRON_TEMPLATE_URL: &str =
    "https://github.com/ottofeller/kinetics-cron-template/archive/refs/heads/main.zip";

const ENDPOINT_TEMPLATE_URL: &str =
    "https://github.com/ottofeller/kinetics-endpoint-template/archive/refs/heads/main.zip";

const WORKER_TEMPLATE_URL: &str =
    "https://github.com/ottofeller/kinetics-worker-template/archive/refs/heads/main.zip";

const GITHUB_DEPLY_TEMPLATE: &str = include_str!("github-workflow-template.yaml");

/// Initialize a new Kinetics project by downloading and unpacking a template archive
///
/// Downloads the Kinetics template archive into a new directory,
/// customizes it with the provided project name, and sets up a ready-to-use project structure.
pub async fn init(
    name: &str,
    function_type: FunctionType,
    is_git_enabled: bool,
) -> eyre::Result<()> {
    let project_dir = env::current_dir()
        .wrap_err(Error::new(
            "Failed to determine current directory",
            Some("Please verify you have proper file system permissions."),
        ))?
        .join(name);

    if project_dir.exists() {
        return Err(Error::new(
            &format!("Directory '{}' already exists", project_dir.display()),
            Some("Choose a different name or delete the existing directory."),
        )
        .into());
    }

    println!(
        "\n{} {} {}...",
        console::style("Starting project").green().bold(),
        console::style("in").dim(),
        console::style(&project_dir.to_string_lossy()).bold()
    );

    // Create project directory
    fs::create_dir_all(&project_dir)
        .inspect_err(|e| log::error!("{e:?}"))
        .wrap_err(Error::new(
            "Failed to create project directory",
            Some("Please verify you have proper file system permissions."),
        ))?;

    print!(
        "\r\x1B[K{}",
        console::style("Downloading template archive").dim()
    );

    let client = reqwest::Client::new();

    let template_url = match function_type {
        FunctionType::Cron => CRON_TEMPLATE_URL,
        FunctionType::Worker => WORKER_TEMPLATE_URL,
        FunctionType::Endpoint => ENDPOINT_TEMPLATE_URL,
    };

    let response = match client.get(template_url).send().await {
        Ok(resp) => {
            if !resp.status().is_success() {
                log::error!("Server returned errors: {:?}", resp);

                return halt(
                    "Failed to download template archive",
                    "Please check your internet connection and try again",
                    &project_dir,
                );
            }

            resp
        }
        Err(e) => {
            log::error!("Failed to download archive: {:?}", e);

            return halt(
                "Failed to download template archive",
                "Please check your internet connection and try again",
                &project_dir,
            );
        }
    };

    print!("\r\x1B[K{}", console::style("Extracting template").dim());

    if let Err(_) = unpack(response, &project_dir).await {
        return halt(
            "Failed to unpack template archive",
            "Check if tar is installed and you have enough FS permissions, and try again.",
            &project_dir,
        );
    };

    print!("\r\x1B[K{}", console::style("Cleaning up").dim());

    // The extraction creates a subdirectory with the repository name and branch
    // We need to move all files from that subdirectory to our project directory
    let extracted_dir = project_dir.join(
        template_url
            .replace("https://github.com/ottofeller/", "")
            .replace("/archive/refs/heads/main.zip", "-main"),
    );

    // Move all files from extracted directory to project directory using bash command
    let status = Command::new("bash")
        .args([
            "-c",
            &format!(
                "mv {}/* {}",
                extracted_dir.to_string_lossy(),
                project_dir.to_string_lossy()
            ),
        ])
        .status()
        .inspect_err(|e| log::error!("Can't move files: {:?}", e))
        .wrap_err(Error::new(
            "Failed to move template files",
            Some("Make sure you have proper permissions to execute bash commands."),
        ))?;

    if !status.success() {
        log::error!("Can't move files: {:?}", status);

        return halt(
            "Failed to move template files",
            "The bash command failed. Check file permissions.",
            &project_dir,
        );
    }

    // Remove the now empty extracted directory
    fs::remove_dir_all(&extracted_dir).unwrap_or(());

    print!("\r\x1B[K{}", console::style("Renaming project").dim());

    rename(&project_dir, name).wrap_err(Error::new(
        "Failed to update Cargo.toml",
        Some("Template might be corrupted (reach us at support@usekinetics.com), or check file system permissions."),
    ))?;

    print!("\r\x1B[K");
    if is_git_enabled {
        init_git(&project_dir)?;
    }

    println!("{}", console::style("Done").cyan());
    Ok(())
}

/// Updates the project name in Cargo.toml
fn rename(project_dir: &Path, name: &str) -> eyre::Result<()> {
    let cargo_toml_path = project_dir.join("Cargo.toml");

    let cargo_toml_content =
        fs::read_to_string(&cargo_toml_path).inspect_err(|e| log::error!("Can't read: {e:?}"))?;

    // Parse the content as a TOML document
    let mut doc = cargo_toml_content
        .parse::<DocumentMut>()
        .inspect_err(|e| log::error!("Can't parse: {e:?}"))?;

    let Some(package) = doc.get_mut("package") else {
        log::error!("Missing [package] section");
        return Err(eyre!("Invalid Cargo.toml format"));
    };

    // Update the package name in [package] section
    let Some(package_table) = package.as_table_mut() else {
        log::error!("Cargo.toml:package is not a table");
        return Err(eyre!("Invalid Cargo.toml format"));
    };

    package_table["name"] = value(name);

    // Write the updated content back
    let updated_content = doc.to_string();

    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&cargo_toml_path)
        .inspect_err(|e| log::error!("Can't open: {e:?}"))?;

    file.write_all(updated_content.as_bytes())
        .inspect_err(|e| log::error!("Can't write: {e:?}"))?;

    Ok(())
}

/// Unpack gzip bytes received from GitHub
async fn unpack(response: Response, project_dir: &Path) -> eyre::Result<()> {
    let archive_bytes = response
        .bytes()
        .await
        .inspect_err(|e| log::error!("Failed to read archive data: {:?}", e))?;

    log::info!("Extracting template files...");

    // Create a temporary file for the archive
    let temp_file_path = project_dir.join("template.tar.gz");

    let mut temp_file = fs::File::create(&temp_file_path)
        .inspect_err(|e| log::error!("Can't create tmp file: {e:?}"))?;

    // Write the archive content to the temporary file
    temp_file
        .write_all(&archive_bytes)
        .inspect_err(|e| log::error!("Can't write to tmp file: {e:?}"))?;

    // Extract the archive using the system tar command
    let status = Command::new("tar")
        .args([
            "xzf",
            &temp_file_path.to_string_lossy(),
            "-C",
            &project_dir.to_string_lossy(),
        ])
        .status()
        .inspect_err(|e| log::error!("Can't run tar command: {:?}", e))?;

    // Clean up the temporary file
    fs::remove_file(&temp_file_path).unwrap_or(());

    if !status.success() {
        log::error!("Can't unpack: {:?}", status);
        return Err(eyre!("Failed to extract template archive"));
    }

    Ok(())
}

/// Setup git and github workflow for automatic deployments
fn init_git(project_dir: &Path) -> eyre::Result<()> {
    // Do not init git if it's already there
    let is_repo = Command::new("git")
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|exit_status| exit_status.success())
        .unwrap_or_default();

    if is_repo {
        return Ok(());
    }

    log::info!("No git repo found. Init a new one.");

    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&project_dir)
        .status()
        .inspect_err(|e| log::error!("Can't init git: {:?}", e))
        .wrap_err(Error::new(
            "Failed to init git",
            Some("Make sure you have proper permissions."),
        ))?;

    if !status.success() {
        log::error!("Can't init git: {:?}", status);

        return halt(
            "Failed to init git",
            "Failed to init git . Check file permissions.",
            &project_dir,
        );
    }

    // Add a github CD workflow
    let deploy_workflow = GITHUB_DEPLY_TEMPLATE.replace("PLACEHOLDER_DIR_PATH", ".");
    let workflow_dir = project_dir.join(".github/workflows");
    fs::create_dir_all(&workflow_dir)
        .inspect_err(|e| log::error!("{e:?}"))
        .wrap_err(Error::new(
            "Failed to create github workflows directory",
            Some("Check file system permissions."),
        ))?;
    let deploy_workflow_path = workflow_dir.join("kinetics.yaml");
    fs::write(deploy_workflow_path, deploy_workflow).wrap_err(Error::new(
        "Failed to write deploy workflow file",
        Some("Check file system permissions."),
    ))?;

    println!("{}", console::style("A github workflow for continious deployment was added to the project. Make sure to pull a token and save it to the repo under KINETICS_TOKEN name in order to properly authenticate the workflow. For details see https://github.com/ottofeller/kinetics/blob/main/README.md#deploy-from-github-actions").dim());

    Ok(())
}

/// Clean up, and throw an error
fn halt(message: &str, details: &str, dir: &Path) -> eyre::Result<()> {
    print!("\r\x1B[K");
    fs::remove_dir_all(dir).unwrap_or(());
    Err(Error::new(message, Some(details)).into())
}
