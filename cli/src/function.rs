use crate::client::Client;
use crate::crat::Crate;
use crate::deploy::DeployConfig;
use crate::error::Error;
use eyre::{eyre, ContextCompat, OptionExt, WrapErr};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;
use zip::write::SimpleFileOptions;
use tokio::io::AsyncBufReadExt;
use crate::build::pipeline::Progress;
use tokio::io::{BufReader};
use std::process::Stdio;

#[derive(Clone, Debug)]
pub struct Function {
    pub id: String,
    pub path: PathBuf,
    pub s3key_encrypted: Option<String>,

    // Original parent crate
    pub crat: Crate,
}

impl Function {
    pub fn new(path: &Path) -> eyre::Result<Self> {
        Ok(Function {
            id: uuid::Uuid::new_v4().into(),
            path: path.to_path_buf(),
            crat: Crate::new(path.to_path_buf())?,
            s3key_encrypted: None,
        })
    }

    /// Try to find a function by name in the vec of functions
    pub fn find_by_name(functions: &Vec<Function>, name: &str) -> eyre::Result<Function> {
        functions
            .iter()
            .find(|f| name.eq(&f.name().unwrap()))
            .wrap_err("No function with such name")
            .cloned()
    }

    pub fn set_s3key_encrypted(&mut self, s3key_encrypted: String) {
        self.s3key_encrypted = Some(s3key_encrypted);
    }

    pub fn s3key_encrypted(&self) -> Option<String> {
        self.s3key_encrypted.clone()
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
            .get("function")
            .wrap_err("No [function]")
            .cloned()
    }

    /// User defined name of the function
    pub fn name(&self) -> eyre::Result<String> {
        let error = Error::new(
            "Missing the \"name\" property in Cargo.toml",
            Some("Try to rebuild the project."),
        );

        Ok(self
            .meta()?
            .get("name")
            .wrap_err(error.clone())?
            .as_str()
            .wrap_err(error)?
            .to_string())
    }

    pub async fn build(&self, logger: &Progress) -> eyre::Result<()> {
        let mut cmd = tokio::process::Command::new("cargo")
            .arg("lambda")
            .arg("build")
            .arg("--release")
            .arg("--lambda-dir")
            .arg("target/lambda")
            .current_dir(&self.path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .wrap_err("Failed to execute the process")?;

        let mut is_failed = false;
        let mut error_message_lines = Vec::new();

        if let Some(stderr) = cmd.stderr.take() {
            let mut reader = BufReader::new(stderr).lines();

            while let Some(line) = reader.next_line().await? {
                if line.trim().starts_with("error") || is_failed {
                    is_failed = true;
                    error_message_lines.push(line);
                    continue;
                }

                if line.trim().is_empty() {
                    logger.progress_bar.set_message(
                        format!(
                            "{} {}",
                            console::style("    Building").green().bold(),
                            self.name()?,
                        ));
                } else {
                    logger.progress_bar.set_message(
                        format!(
                            "{} {} {}",
                            console::style("    Building").green().bold(),
                            self.name()?,
                            console::style(format!("({})", line.trim())).dim()
                        ));
                }
            }
        }

        logger.progress_bar.finish_and_clear();
        let status = cmd.wait().await?;

        if !status.success() {
            return Err(eyre!("{}", error_message_lines.join("\n")));
        }

        Ok(())
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
        self.path.join(format!("{}.zip", self.id))
    }

    pub fn toml_string(&self) -> eyre::Result<String> {
        std::fs::read_to_string(self.path.join("Cargo.toml"))
            .wrap_err("Failed to read function's Cargo.toml")
    }

    fn build_path(&self) -> PathBuf {
        self.path
            .join("target")
            .join("lambda")
            .join("bootstrap")
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
            s3key_encrypted: String,
        }

        let path = self.bundle_path();

        let presigned = client
            .post("/upload")
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

        self.set_s3key_encrypted(presigned.s3key_encrypted);
        Ok(())
    }

    /// Return true if the function is the only supposed for local invocations
    pub fn is_local(&self) -> eyre::Result<bool> {
        if self.meta().is_err() {
            return Err(eyre!("Could not get function's meta"));
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
            .get("environment")
            .wrap_err("No [environment]")
            .cloned()?
            .as_table()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect::<HashMap<String, String>>())
    }
}
