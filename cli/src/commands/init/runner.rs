use crate::commands::cicd::github;
use crate::commands::init::InitCommand;
use crate::error::Error;
use crate::function::Type as FunctionType;
use crate::project::Project;
use crate::runner::Runner;
use eyre::{eyre, WrapErr};
use reqwest::Response;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use toml_edit::{value, DocumentMut};

const CRON_TEMPLATE_URL: &str =
    "https://github.com/ottofeller/kinetics-cron-template/archive/refs/heads/main.zip";

const ENDPOINT_TEMPLATE_URL: &str =
    "https://github.com/ottofeller/kinetics-endpoint-template/archive/refs/heads/main.zip";

const WORKER_TEMPLATE_URL: &str =
    "https://github.com/ottofeller/kinetics-worker-template/archive/refs/heads/main.zip";

pub(crate) struct InitRunner {
    pub(super) command: InitCommand,
    pub(super) dir: PathBuf,
}

impl Runner for InitRunner {
    /// Initialize a new Kinetics project by downloading and unpacking a template archive
    ///
    /// Downloads the Kinetics template archive into a new directory,
    /// customizes it with the provided project name, and sets up a ready-to-use project structure.
    async fn run(&mut self) -> Result<(), Error> {
        let function_type = if self.command.cron {
            FunctionType::Cron
        } else if self.command.worker {
            FunctionType::Worker
        } else {
            FunctionType::Endpoint
        };

        let is_git_enabled = !self.command.no_git;
        self.set_dir()?;

        println!(
            "\n{} {} {}...",
            console::style("Starting project").green().bold(),
            console::style("in").dim(),
            console::style(&self.dir.to_string_lossy()).bold()
        );

        // Create project directory
        fs::create_dir_all(&self.dir)
            .wrap_err("Failed to create project directory")
            .map_err(|e| self.error(None, None, Some(e.into())))?;

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
                    log::error!("Template server returned error: {resp:?}");
                    self.cleanup();
                    return Err(self.server_error(None));
                }

                resp
            }
            Err(e) => {
                log::error!("Request to template server failed: {e:?}");
                self.cleanup();
                return Err(self.server_error(None));
            }
        };

        print!("\r\x1B[K{}", console::style("Extracting template").dim());
        let unpack_result = self.unpack(response).await;

        if unpack_result.is_err() {
            self.cleanup();

            return Err(self.error(
                Some("Failed to unpack template archive"),
                Some("Check if tar is installed and you have enough FS permissions."),
                Some(unpack_result.err().unwrap().into()),
            ));
        };

        print!("\r\x1B[K{}", console::style("Cleaning up").dim());

        // The extraction creates a subdirectory with the repository name and branch
        // We need to move all files from that subdirectory to our project directory
        let extracted_dir = self.dir.join(
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
                    self.dir.to_string_lossy()
                ),
            ])
            .status()
            .wrap_err("Failed to move template files")
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        if !status.success() {
            self.cleanup();

            return Err(self.error(
                Some("Failed to move template files"),
                Some("The bash command failed. Check file permissions."),
                None,
            ));
        }

        // Remove the now empty extracted directory
        fs::remove_dir_all(&extracted_dir).unwrap_or(());

        print!("\r\x1B[K{}", console::style("Renaming project").dim());

        self.rename(&self.command.name)
            .map_err(|e| self.error(
                Some("Failed to update Cargo.toml"),
                Some("Template might be corrupted (reach us at support@kineticscloud.com), or check file system permissions."),
                Some(e.into())
            ))?;

        print!("\r\x1B[K");

        if is_git_enabled {
            self.init_git().map_err(|e| {
                self.cleanup();
                self.error(None, None, Some(e.into()))
            })?;
        }

        println!("{}", console::style("Done").bold().green());
        Ok(())
    }
}

impl InitRunner {
    // Set the dir to create project in
    fn set_dir(&mut self) -> Result<(), Error> {
        let dir = env::current_dir()
            .wrap_err("Failed to determine current directory")
            .map_err(|e| self.error(None, None, Some(e.into())))?
            .join(&self.command.name);

        if dir.exists() {
            return Err(self.error(
                Some(&format!("Directory '{}' already exists", dir.display())),
                Some("Choose a different name or delete the existing directory."),
                None,
            ));
        }

        self.dir = dir;
        Ok(())
    }

    /// Clean up by deleting the dir with the new project
    fn cleanup(&self) -> () {
        fs::remove_dir_all(&self.dir).unwrap_or(())
    }

    /// Updates the project name in Cargo.toml
    fn rename(&self, name: &str) -> eyre::Result<()> {
        let cargo_toml_path = self.dir.join("Cargo.toml");

        let cargo_toml_content = fs::read_to_string(&cargo_toml_path)
            .inspect_err(|e| log::error!("Can't read: {e:?}"))?;

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
    async fn unpack(&self, response: Response) -> eyre::Result<()> {
        let archive_bytes = response
            .bytes()
            .await
            .inspect_err(|e| log::error!("Failed to read archive data: {e:?}"))?;

        log::info!("Extracting template files...");

        // Create a temporary file for the archive
        let temp_file_path = self.dir.join("template.tar.gz");

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
                &self.dir.to_string_lossy(),
            ])
            .status()
            .inspect_err(|e| log::error!("Can't run tar command: {e:?}"))?;

        // Clean up the temporary file
        fs::remove_file(&temp_file_path).unwrap_or(());

        if !status.success() {
            log::error!("Can't unpack: {status:?}");
            return Err(eyre!("Failed to extract template archive"));
        }

        Ok(())
    }

    /// Setup git and github workflow for automatic deployments
    fn init_git(&self) -> eyre::Result<()> {
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
            .current_dir(&self.dir)
            .status()
            .inspect_err(|e| log::error!("Can't init git: {e:?}"))
            .wrap_err(Error::new(
                "Failed to init git",
                Some("Make sure you have proper permissions."),
            ))?;

        if !status.success() {
            log::error!("Can't init git: {status:?}");
            return Err(eyre!("Failed to init git"));
        }

        fs::write(self.dir.join(".gitignore"), "target/\n")
            .inspect_err(|e| log::error!("Can't write .gitignore file: {:?}", e))
            .wrap_err(Error::new(
                "Failed to write .gitignore file",
                Some("Check file system permissions."),
            ))?;

        // Add a github CD workflow
        github::workflow(&Project::from_path(self.dir.clone().into())?, true)
    }
}
