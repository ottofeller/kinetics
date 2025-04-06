use eyre::{ContextCompat, Ok, WrapErr};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Crate {
    /// Path to the crate's root directory
    pub path: PathBuf,
    pub name: String,
    pub toml: toml::Value,
    pub toml_string: String,
}

impl Crate {
    pub fn new(path: PathBuf) -> eyre::Result<Self> {
        let cargo_toml_path = path.join("Cargo.toml");
        let cargo_toml_string = std::fs::read_to_string(&cargo_toml_path)
            .wrap_err(format!("Failed to read Cargo.toml: {cargo_toml_path:?}"))?;

        let cargo_toml: toml::Value = cargo_toml_string
            .parse::<toml::Value>()
            .wrap_err("Failed to parse Cargo.toml")?;

        Ok(Crate {
            path,
            name: cargo_toml
                .get("package")
                .and_then(|pkg| pkg.get("name"))
                .and_then(|name| name.as_str())
                .wrap_err("Failed to get crate name from Cargo.toml")?
                .to_string(),

            toml: cargo_toml,
            toml_string: cargo_toml_string,
        })
    }
}
