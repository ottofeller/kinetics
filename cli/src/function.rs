use crate::crat::Crate;
use eyre::{eyre, ContextCompat, Ok, WrapErr};
use std::path::PathBuf;
use zip::write::SimpleFileOptions;

#[derive(Clone)]
pub struct Function {
    pub id: String,
    pub path: PathBuf,

    // Oringal parent crate
    pub crat: Crate,
}

impl Function {
    pub fn new(path: &PathBuf) -> eyre::Result<Self> {
        Ok(Function {
            id: uuid::Uuid::new_v4().into(),
            path: path.clone(),
            crat: Crate::new(path.clone())?,
        })
    }

    pub fn environment(&self) -> eyre::Result<toml::Value> {
        self.crat
            .toml
            .get("package")
            .wrap_err("No [package]")?
            .get("metadata")
            .wrap_err("No [metadata]")?
            .get("sky")
            .wrap_err("No [sky]")?
            .get("environment")
            .wrap_err("No [environment]")
            .cloned()
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

    /// URL path to be used in the CloudFront distribution
    ///
    /// Optional property, only available for endpoint type of functions.
    pub fn url_path(&self) -> Option<String> {
        if self.meta().is_err() || self.role().unwrap() != "endpoint" {
            return None;
        }

        let meta = self.meta().unwrap();

        if meta.get("url_path").is_none() {
            return None;
        }

        Some(meta.get("url_path").unwrap().as_str().unwrap().to_string())
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

    pub fn role(&self) -> eyre::Result<String> {
        Ok(self
            .meta()?
            .get("role")
            .wrap_err("No [role]")?
            .as_str()
            .wrap_err("Not a string")?
            .to_string())
    }

    pub fn build(&self) -> eyre::Result<()> {
        println!("Building {:?} with cargo-lambda...", self.path);

        let status = std::process::Command::new("cargo")
            .arg("lambda")
            .arg("build")
            .arg("--release")
            .current_dir(&self.path)
            .output()
            .wrap_err("Failed to execute the process")?
            .status;

        if !status.success() {
            return Err(eyre!("Build failed: {:?} {:?}", status.code(), self.path));
        }

        Ok(())
    }

    pub fn bundle(&self) -> eyre::Result<()> {
        println!("Bundling {:?}...", self.path);
        let file = std::fs::File::create(&self.bundle_path())?;
        let mut zip = zip::ZipWriter::new(file);

        let mut f = std::fs::File::open(
            self.build_path()?
                .to_str()
                .ok_or(eyre!("Failed to construct bundle path"))?,
        )?;

        zip.start_file("bootstrap", SimpleFileOptions::default())?;
        std::io::copy(&mut f, &mut zip)?;
        zip.finish()?;
        Ok(())
    }

    pub fn bundle_path(&self) -> PathBuf {
        self.path.join(format!("{}.zip", self.id))
    }

    pub fn bundle_name(&self) -> String {
        format!("{}.zip", self.id)
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

    pub fn resources(&self) -> Vec<&crate::Resource> {
        self.crat.resources.iter().clone().collect()
    }
}
