use crate::api::client::Client;
use crate::api::error::ApiError;
use crate::config::deploy::DeployConfig;
use crate::error::Error;
use crate::project::Project;
use crate::secrets::Secrets;
use base64::Engine as _;
use crc_fast::{CrcAlgorithm::Crc64Nvme, Digest};
use eyre::{eyre, ContextCompat, WrapErr};
use kinetics_api::{func, upload};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};

mod bundle;

// Re-export types from kinetics-parser
pub use kinetics_parser::{Params, ParsedFunction, Role};

/// Represents a function in the project
#[derive(Clone, Debug)]
pub struct Function {
    /// The name of the function
    pub name: String,

    /// Source package path within the generated workspace.
    pub package_path: PathBuf,

    /// Whether the function is requested for deployment
    pub is_deploying: bool,

    /// The role of the workload
    pub role: Role,

    /// The workload-specific parameters of the function
    pub params: Params,

    /// The project that contains the function, it belongs to Crate in the build directory
    pub project: Project,

    /// Function-level secrets from the workspace member `.env.secrets`, if any.
    pub secrets: Option<kinetics_api::stack::deploy::FunctionSecrets>,
}

impl Function {
    /// Instantiate struct from parsed function data
    pub fn new(project: &Project, function: &ParsedFunction) -> eyre::Result<Self> {
        let secrets = if !project.workspace.is_standalone_crate && project.is_ws_root() {
            // Within workspace if package the function belongs to is not a project,
            // read secrets from package and write them to the function.
            Secrets::from_files(&[&project.workspace.root_path.join(&function.pkg_rel_path)]).map(
                |values| kinetics_api::stack::deploy::FunctionSecrets {
                    scope: function.pkg_name.clone(),
                    values,
                },
            )
        } else {
            // In standalone crate or workspace-root project
            // all secrets are global and reside in project, nothing in function.
            None
        };

        // Resolve local fn name in `dlq` into full function name.
        let mut params = function.params.clone();
        if let Params::Worker(worker) = &mut params {
            if let Some(target) = worker.dlq.as_deref() {
                worker.dlq = Some(function.full_name(target)?);
            }
        }

        Ok(Function {
            name: function.func_name(false)?,
            package_path: function.pkg_rel_path.clone(),
            is_deploying: false,
            project: project.clone(),
            params,
            role: function.role.clone(),
            secrets,
        })
    }

    /// Try to find a function by name in the vec of functions
    pub fn find_by_name(functions: &[Function], name: &str) -> eyre::Result<Function> {
        functions
            .iter()
            .find(|f| name.eq(&f.name))
            .wrap_err("No function with such name")
            .cloned()
    }

    pub fn set_is_deploying(mut self, is_deploying: bool) -> Self {
        self.is_deploying = is_deploying;
        self
    }

    /// A path to the cached Lambda deployment bundle
    pub fn bundle_path(&self) -> eyre::Result<PathBuf> {
        Ok(self
            .project
            .build_path()?
            .join("target")
            .join("lambda")
            .join(&self.name)
            .join("bootstrap.zip"))
    }

    /// Call the /upload endpoint to get the presigned URL and upload the file
    /// Returns a boolean indicating whether the resource has been updated.
    pub async fn upload(
        &mut self,
        client: &Client,
        deploy_config: Option<&dyn DeployConfig>,
    ) -> eyre::Result<bool> {
        if let Some(config) = deploy_config {
            return config.upload(self).await;
        }

        let path = self.bundle_path()?;
        let data = tokio::fs::read(&path).await?;

        let mut digest = Digest::new(Crc64Nvme);
        digest.update(&data);
        let body = upload::Request {
            project_name: self.project.name.clone(),
            name: self.name.clone(),
            checksum: base64::prelude::BASE64_STANDARD.encode(digest.finalize().to_be_bytes()),
            org: self.project.org.clone(),
        };

        log::debug!(
            "Calling /upload with body:\n{}",
            serde_json::to_string_pretty(&body)?
        );

        let response = client
            .post("/upload")
            .json(&body)
            .send()
            .await
            .inspect_err(|e| log::error!("Upload request failed: {e:?}"))?;

        // If the file has not changed, skip the upload.
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(false);
        }

        if !response.status().is_success() {
            return Err(ApiError::from_response(response).await?.into());
        }

        let presigned: upload::Response = response
            .json()
            .await
            .wrap_err(Error::new("Server error", Some("Try again later.")))?;

        let public_client = reqwest::Client::new();

        public_client
            .put(&presigned.url)
            .body(data)
            .send()
            .await?
            .error_for_status()?;

        Ok(true)
    }

    /// Env vars to be added to function's runtime
    ///
    /// These are env vars assigned to the function in macro definition
    /// as well as those defined globally in .env file.
    pub fn environment(&self) -> HashMap<String, String> {
        let mut env = self.project.environment().clone();
        env.extend(self.params.environment().clone());
        env
    }

    /// URL to call the function
    ///
    /// Only relevant for endpoint type of functions.
    pub async fn url(&self) -> eyre::Result<String> {
        let url_path = match &self.params {
            Params::Endpoint(endpoint) => Ok(&endpoint.url_path),
            _ => Err(eyre!("Not an endpoint")),
        }?;

        Ok(format!(
            "{}{}",
            Project::fetch_one(&self.project.name, self.project.org.as_deref())
                .await?
                .url(),
            url_path
        ))
    }

    /// Get the function deployment status from the backend
    pub async fn status(&self, client: &Client) -> eyre::Result<Option<String>> {
        let result = client
            .post("/function/status")
            .json(&func::status::Request {
                project: (&self.project).into(),
                function_name: func::status::FunctionName::Single(self.name.clone()),
            })
            .send()
            .await?;

        if !result.status().is_success() {
            return Err(ApiError::from_response(result).await?.into());
        }

        let result: func::status::Response = result
            .json()
            .await
            .inspect_err(|err| log::debug!("Invalid function status response: {err:?}"))
            .wrap_err(Error::new("Server error", Some("Try again later.")))?;

        match result.last_modified {
            func::status::FunctionStatus::Single(status) => match status {
                func::status::SingleFunctionStatus::Modified(dt) => Ok(Some(dt)),
                func::status::SingleFunctionStatus::NotModified => Ok(None),
                func::status::SingleFunctionStatus::NotFound => {
                    log::debug!(
                        "Function not found or not deployed yet: {}/{}",
                        self.project.name,
                        self.name
                    );

                    Ok(None)
                }
                func::status::SingleFunctionStatus::Error(e) => Err(Error::new(
                    &format!(
                        "Function status request failed for {}/{}",
                        self.project.name, self.name
                    ),
                    Some(&e),
                )
                .into()),
            },
            func::status::FunctionStatus::List(_) => Err(Error::new(
                &format!(
                    "Unexpected response for function {}/{} status",
                    self.project.name, self.name
                ),
                Some("Please report a bug at support@deploykinetics.com"),
            )
            .into()),
        }
    }
}

/// Get from the backend deployment status of multiple functions
pub async fn status(
    client: &Client,
    project: &Project,
    functions: &[Function],
) -> eyre::Result<Vec<String>> {
    let result = client
        .post("/function/status")
        .json(&func::status::Request {
            project: project.into(),
            function_name: func::status::FunctionName::List(
                functions.iter().map(|f| f.name.to_owned()).collect(),
            ),
        })
        .send()
        .await?;

    if !result.status().is_success() {
        return Err(ApiError::from_response(result).await?.into());
    }

    let result: func::status::Response = result
        .json()
        .await
        .inspect_err(|err| log::debug!("Invalid function statuses response: {err:?}"))
        .wrap_err(Error::new("Server error", Some("Try again later.")))?;

    match result.last_modified {
        func::status::FunctionStatus::Single(_) => Err(Error::new(
            &format!("Unexpected response for {} functions status", project.name),
            Some("Please report a bug at support@deploykinetics.com"),
        )
        .into()),
        func::status::FunctionStatus::List(items) => Ok(items
            .into_iter()
            .map(|status| match status {
                func::status::SingleFunctionStatus::Modified(dt) => dt,
                func::status::SingleFunctionStatus::NotModified => "Not modified".to_string(),
                func::status::SingleFunctionStatus::NotFound => "Not found".to_string(),
                func::status::SingleFunctionStatus::Error(e) => format!("Error {e}"),
            })
            .collect()),
    }
}

pub async fn build(
    functions: &[Function],
    total_progress: &indicatif::ProgressBar,
) -> eyre::Result<()> {
    let Some(Function { project, .. }) = functions.iter().next() else {
        return Err(eyre!("Attempted to build an empty function list"));
    };

    total_progress.set_message("Starting cargo...");
    let build_path = project.build_path()?;
    let debug_build = log::log_enabled!(log::Level::Debug);
    let show_build_output = debug_build || !std::io::stderr().is_terminal();
    let mut cmd = tokio::process::Command::new("cargo");
    cmd.arg("lambda")
        .arg("build")
        .arg("--workspace")
        .arg("--release")
        .arg("--target")
        .arg("x86_64-unknown-linux-musl")
        .arg("--output-format")
        .arg("binary")
        .current_dir(&build_path)
        // Null stdout avoids an unread pipe when cargo-lambda inherits verbose RUST_LOG.
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    if debug_build {
        // Cargo's verbose output includes Fresh/Dirty units and their rebuild reasons.
        cmd.arg("--verbose").arg("--timings");
    }

    for function in functions {
        cmd.arg("--bin").arg(&function.name);
    }

    log::debug!(
        "Building {} Lambda binaries in {}",
        functions.len(),
        build_path.display()
    );
    log::debug!("Running {cmd:?}");
    let compile_started_at = Instant::now();
    let mut child = cmd.spawn().wrap_err("Failed to execute the process")?;

    let mut is_failed = false;
    let mut error_message_lines = Vec::new();

    if let Some(stderr) = child.stderr.take() {
        let mut reader = BufReader::new(stderr).lines();
        let regex = regex::Regex::new(r"^[\t ]+")?;

        while let Some(line) = reader.next_line().await? {
            if show_build_output {
                total_progress.suspend(|| eprintln!("{line}"));
            }

            let trimmed = line.trim();

            if is_failed || trimmed.starts_with("error") || trimmed.starts_with("Error") {
                is_failed = true;
                error_message_lines.push(line);
                continue;
            };
            total_progress.set_message(regex.replace_all(&line, "").to_string());
        }
    }

    total_progress.set_message("");
    let status = child.wait().await?;
    log::debug!(
        "Cargo finished in {:.2}s ({status})",
        compile_started_at.elapsed().as_secs_f64()
    );

    if !status.success() {
        return Err(eyre!("{}", error_message_lines.join("\n")));
    }

    let functions = functions.to_vec();

    let bundle_started_at = Instant::now();
    tokio::task::spawn_blocking(move || bundle::bundle_functions(&functions))
        .await
        .wrap_err("Failed to join the Lambda bundling task")??;
    log::debug!(
        "Lambda ZIP packaging finished in {:.2}s",
        bundle_started_at.elapsed().as_secs_f64()
    );

    Ok(())
}

impl From<&Function> for kinetics_api::stack::deploy::FunctionRequest {
    fn from(function: &Function) -> Self {
        Self {
            name: function.name.clone(),
            is_deploying: function.is_deploying,
            params: function.params.clone(),
            role: function.role.clone(),
            environment: function.environment(),
            secrets: function.secrets.clone().filter(|s| !s.values.is_empty()),
        }
    }
}
