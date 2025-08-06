use crate::client::Client;
use crate::config::build_config;
use crate::crat::Crate;
use crate::deploy::DeployConfig;
use crate::error::Error;
use eyre::{eyre, ContextCompat, OptionExt, WrapErr};
use kinetics_parser::{ParsedFunction, Parser};
use reqwest::StatusCode;
use serde_json::json;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use syn::visit::Visit;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;

pub enum Type {
    Cron,
    Endpoint,
    Worker,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub id: String,
    pub name: String,
    pub is_deploying: bool,

    // Original parent crate
    pub crat: Crate,
}

impl Function {
    pub fn new(crate_path: &Path, name: &str) -> eyre::Result<Self> {
        Ok(Function {
            id: uuid::Uuid::new_v4().into(),
            name: name.into(),
            is_deploying: false,
            crat: Crate::new(crate_path.to_path_buf())?,
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

    fn meta(&self) -> eyre::Result<toml::Value> {
        self.crat
            .toml
            .get("package")
            .wrap_err("No [package]")?
            .get("metadata")
            .wrap_err("No [metadata]")?
            .get("kinetics")
            .wrap_err("No [kinetics]")?
            .get(&self.name)
            .wrap_err(format!("No [{}]", self.name))?
            .get("function")
            .wrap_err("No [function]")
            .cloned()
    }

    pub fn is_deploying(mut self, is_deploying: bool) -> Self {
        self.is_deploying = is_deploying;
        self
    }

    pub async fn bundle(&self) -> eyre::Result<()> {
        let path = self.build_path();

        let path = path
            .to_str()
            .ok_or_eyre("Failed to construct bundle path")?;

        let mut f = tokio::fs::File::open(path)
            .await
            .wrap_err(format!("Could not open the file \"{path}\""))?;

        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer).await?;

        let bundle_path = self.bundle_path();

        // Zip crate doesn't have async support, so we have to use a blocking task here
        tokio::task::spawn_blocking(move || -> eyre::Result<()> {
            let file = std::fs::File::create(bundle_path)?;
            let mut zip = zip::ZipWriter::new(file);

            zip.start_file("bootstrap", SimpleFileOptions::default())
                .wrap_err("Could not open ZIP file")?;

            zip.write_all(&buffer)
                .wrap_err("Could not write to ZIP file")?;

            zip.finish().wrap_err("Could not close ZIP file")?;
            Ok(())
        })
        .await
        .wrap_err("Failed to spawn the blocking task")?
        .wrap_err("Failed to create a Zip archive")?;

        Ok(())
    }

    pub fn bundle_path(&self) -> PathBuf {
        self.crat.path.join(format!("{}.zip", self.id))
    }

    fn build_path(&self) -> PathBuf {
        self.crat
            .path
            .join("target")
            .join("lambda")
            .join(&self.name)
            .join("bootstrap")
    }

    /// Call the /upload endpoint to get the presigned URL and upload the file
    pub async fn upload(
        &mut self,
        client: &Client,
        deploy_config: Option<&dyn DeployConfig>,
    ) -> eyre::Result<()> {
        if let Some(config) = deploy_config {
            return config.upload(self).await;
        }

        #[derive(serde::Deserialize, Debug)]
        struct PreSignedUrl {
            url: String,
        }

        let path = self.bundle_path();

        let presigned = client
            .post("/upload")
            .body(json!({"name": self.name}).to_string())
            .send()
            .await?
            .json::<PreSignedUrl>()
            .await?;

        let public_client = reqwest::Client::new();

        public_client
            .put(&presigned.url)
            .body(tokio::fs::read(&path).await?)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    /// Return true if the function is the only supposed for local invocations
    pub fn _is_local(&self) -> eyre::Result<bool> {
        if self.meta().is_err() {
            return Err(eyre!("Could not get function's meta {}", self.name,));
        }

        Ok(self
            .meta()?
            .get("is_local")
            .unwrap_or(&toml::Value::Boolean(false))
            .as_bool()
            .unwrap_or(false))
    }

    /// Return env vars assigned to the function in macro definition
    pub fn environment(&self) -> eyre::Result<HashMap<String, String>> {
        Ok(self
            .crat
            .toml
            .get("package")
            .wrap_err("No [package]")?
            .get("metadata")
            .wrap_err("No [metadata]")?
            .get("kinetics")
            .wrap_err("No [kinetics]")?
            .get(&self.name)
            .wrap_err(format!("No [{}]", self.name))?
            .get("environment")
            .wrap_err("No [environment]")
            .cloned()?
            .as_table()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect::<HashMap<String, String>>())
    }

    /// URL to call the function
    ///
    /// Only relevant for endpoint type of functions.
    pub fn url(&self) -> eyre::Result<String> {
        let path = self
            .meta()?
            .get("url_path")
            .wrap_err("No URL path specified for the function (not an enpoint?)")?
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_default();

        Ok(format!(
            "https://{}.{}{}",
            self.crat.escaped_name(),
            build_config()?.domain,
            path
        ))
    }

    /// Get the function deployment status from the backend
    pub async fn status(&self, client: &Client) -> eyre::Result<Option<String>> {
        #[derive(serde::Serialize)]
        struct JsonBody {
            crate_name: String,
            function_name: String,
        }

        #[derive(serde::Deserialize)]
        struct JsonResponse {
            /// The date and time that the function was last updated
            /// in ISO-8601 format (YYYY-MM-DDThh:mm:ss.sTZD).
            last_modified: Option<String>,
        }

        let result = client
            .post("/function/status")
            .json(&serde_json::json!(JsonBody {
                crate_name: self.crat.name.clone(),
                function_name: self.name.clone(),
            }))
            .send()
            .await
            .inspect_err(|err| log::error!("{err:?}"))
            .wrap_err(Error::new(
                "Network request failed",
                Some("Try again in a few seconds."),
            ))?;

        if result.status() != StatusCode::OK {
            return Err(Error::new(
                &format!(
                    "Function status request failed for {}/{}",
                    self.crat.name.clone(),
                    self.name.clone()
                ),
                Some("Try again in a few seconds."),
            )
            .into());
        }

        let status: JsonResponse = result.json().await.wrap_err("Failed to parse response")?;

        Ok(status.last_modified)
    }
}

/// Parse current project code
/// and return all functions encountered with `kinetics` macro.
pub fn project_functions(crat: &Crate) -> eyre::Result<Vec<ParsedFunction>> {
    // Parse functions from source code
    let mut parser = Parser::new();

    for entry in WalkDir::new(&crat.path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
    {
        let content = std::fs::read_to_string(entry.path())?;
        let syntax = syn::parse_file(&content)?;

        // Set current file relative path for further imports resolution
        // WARN It prevents to implement parallel parsing of files and requires rework in the future
        parser.set_relative_path(entry.path().strip_prefix(&crat.path)?.to_str());

        parser.visit_file(&syntax);
    }

    Ok(parser.functions)
}

pub async fn build(
    functions: &[Function],
    total_progress: &indicatif::ProgressBar,
) -> eyre::Result<()> {
    let Some(Function { crat, .. }) = functions.iter().next() else {
        return Err(eyre!("Attempted to build an empty function list"));
    };

    total_progress.set_message("Starting cargo...");
    let mut cmd = tokio::process::Command::new("cargo");
    cmd.arg("lambda")
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg("x86_64-unknown-linux-musl")
        .current_dir(&crat.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for function in functions {
        cmd.arg("--bin").arg(&function.name);
    }

    let mut child = cmd.spawn().wrap_err("Failed to execute the process")?;

    let mut is_failed = false;
    let mut error_message_lines = Vec::new();

    if let Some(stderr) = child.stderr.take() {
        let mut reader = BufReader::new(stderr).lines();

        while let Some(line) = reader.next_line().await? {
            if line.trim().starts_with("error") || is_failed {
                is_failed = true;
                error_message_lines.push(line);
                continue;
            }

            let regex = regex::Regex::new(r"^[\t ]+").unwrap();
            total_progress.set_message(regex.replace_all(&line, "").to_string());
        }
    }

    total_progress.set_message("");
    let status = child.wait().await?;

    if !status.success() {
        return Err(eyre!("{}", error_message_lines.join("\n")));
    }

    Ok(())
}
