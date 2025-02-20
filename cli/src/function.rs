use crate::crat::Crate;
use aws_sdk_s3::primitives::ByteStream;
use eyre::{eyre, ContextCompat, Ok, WrapErr};
use std::io::Write;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use zip::write::SimpleFileOptions;

#[derive(Clone, Debug)]
pub struct Function {
    pub id: String,
    pub path: PathBuf,
    pub s3key_encrypted: Option<String>,

    // Original parent crate
    pub crat: Crate,
}

impl Function {
    pub fn new(path: &PathBuf) -> eyre::Result<Self> {
        Ok(Function {
            id: uuid::Uuid::new_v4().into(),
            path: path.clone(),
            crat: Crate::new(path.clone())?,
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
    pub async fn zip_stream(&self) -> eyre::Result<ByteStream> {
        aws_sdk_s3::primitives::ByteStream::from_path(self.bundle_path())
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
            .get("sky")
            .wrap_err("No [sky]")?
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
        println!("Building {:?} with cargo-lambda...", self.path);

        let output = tokio::process::Command::new("cargo")
            .arg("lambda")
            .arg("build")
            .arg("--release")
            .current_dir(&self.path)
            .output()
            .await
            .wrap_err("Failed to execute the process")?;

        if !output.status.success() {
            println!(
                "Build failed: {:?}",
                String::from_utf8_lossy(&output.stderr)
            );

            return Err(eyre!(
                "Build failed: {:?} {:?}",
                output.status.code(),
                self.path
            ));
        }

        println!("Function {:?} was built successfully", self.path);
        Ok(())
    }

    pub async fn bundle(&self) -> eyre::Result<()> {
        println!("Bundling {:?}...", self.path);

        let mut f = tokio::fs::File::open(
            self.build_path()?
                .to_str()
                .ok_or(eyre!("Failed to construct bundle path"))?,
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
        .await??;

        Ok(())
    }

    pub fn bundle_path(&self) -> PathBuf {
        self.path.join(format!("{}.zip", self.id))
    }

    pub fn toml_string(&self) -> eyre::Result<String> {
        std::fs::read_to_string(self.path.join("Cargo.toml"))
            .wrap_err("Failed to read function's Cargo.toml")
    }

    fn build_path(&self) -> eyre::Result<PathBuf> {
        Ok(self
            .path
            .join("target")
            .join("lambda")
            .join("bootstrap")
            .join("bootstrap"))
    }
}
