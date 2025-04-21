use crate::crat::Crate;
use aws_sdk_s3::primitives::ByteStream;
use eyre::{eyre, ContextCompat, OptionExt, WrapErr};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;
use zip::write::SimpleFileOptions;

#[cfg(not(feature = "enable-direct-deploy"))]
use crate::client::Client;

#[cfg(feature = "enable-direct-deploy")]
use crate::deploy::DirectDeploy;

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

    pub fn set_s3key_encrypted(&mut self, s3key_encrypted: String) {
        self.s3key_encrypted = Some(s3key_encrypted);
    }

    pub fn s3key_encrypted(&self) -> Option<String> {
        self.s3key_encrypted.clone()
    }

    // Upload the backend manually if the /upload endpoint gets
    // use aws_sdk_s3::primitives::ByteStream;
    #[cfg(feature = "enable-direct-deploy")]
    pub async fn zip_stream(&self) -> eyre::Result<ByteStream> {
        ByteStream::from_path(self.bundle_path())
            .await
            .wrap_err("Failed to read bundled zip")
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
        Ok(self
            .meta()?
            .get("name")
            .wrap_err("No [name]")?
            .as_str()
            .wrap_err("Not a string")?
            .to_string())
    }

    pub async fn build(&self) -> eyre::Result<()> {
        let output = tokio::process::Command::new("cargo")
            .arg("lambda")
            .arg("build")
            .arg("--release")
            .current_dir(&self.path)
            .output()
            .await
            .wrap_err("Failed to execute the process")?;

        if !output.status.success() {
            return Err(eyre!("{}", String::from_utf8_lossy(&output.stderr),));
        }

        Ok(())
    }

    pub async fn bundle(&self) -> eyre::Result<()> {
        let mut f = tokio::fs::File::open(
            self.build_path()
                .to_str()
                .ok_or_eyre("Failed to construct bundle path")?,
        )
        .await?;

        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer).await?;

        let bundle_path = self.bundle_path();

        // Zip crate doesn't have async support, so we have to use a blocking task here
        tokio::task::spawn_blocking(move || -> eyre::Result<()> {
            let file = std::fs::File::create(bundle_path)?;
            let mut zip = zip::ZipWriter::new(file);

            zip.start_file("bootstrap", SimpleFileOptions::default())?;
            zip.write_all(&buffer)?;
            zip.finish()?;

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

    #[cfg(feature = "enable-direct-deploy")]
    pub async fn upload(&mut self, custom_deploy: &dyn DirectDeploy) -> eyre::Result<()> {
        custom_deploy.upload(self).await
    }

    /// Call the /upload endpoint to get the presigned URL and upload the file
    #[cfg(not(feature = "enable-direct-deploy"))]
    pub async fn upload(&mut self, client: &Client) -> eyre::Result<()> {
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
            .meta()
            .unwrap()
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
            .cloned()
            .unwrap()
            .as_table()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect::<HashMap<String, String>>())
    }
}
