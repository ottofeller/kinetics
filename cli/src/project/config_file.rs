use crate::api::projects::Kvdb;
use crate::error::Error;
use crate::project::Project;
use eyre::{ContextCompat, WrapErr};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// FileConfig is the structure of kinetics.toml
#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct ConfigFile {
    /// [project]
    /// name = "some-project"
    #[serde(default)]
    project: ProjectSection,

    /// [[kvdb]]
    /// name = "kvdb"
    #[serde(default)]
    kvdb: Vec<Kvdb>,

    #[serde(skip)]
    path: PathBuf,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ProjectSection {
    name: String,
}

/// FileConfig is the structure of kinetics.toml
impl ConfigFile {
    /// Reads a `FileConfig` instance from a given directory path
    ///
    /// This function looks for a `kinetics.toml` file in the specified directory.
    /// If the `kinetics.toml` file is not present or cannot be read, it returns a default
    /// configuration instead. Additionally, if the `kinetics.toml` file does not explicitly set
    /// the project name, the function will fallback to extracting the name from a `Cargo.toml`
    /// file in the same directory.
    pub(super) fn from_path(path: PathBuf) -> eyre::Result<Self> {
        let config_toml_path = path.join("kinetics.toml");

        let Ok(toml_string) = fs::read_to_string(&config_toml_path) else {
            // Return default config if kinetics.toml is not found
            return Ok(Self {
                project: ProjectSection {
                    name: Self::cargo_toml_name(path.as_path())?,
                },
                path,
                ..Default::default()
            });
        };

        let mut config: ConfigFile =
            toml::from_str(&toml_string).wrap_err("Failed to parse kinetics.toml")?;

        // Set the path to the directory containing kinetics.toml
        config.path = path.clone();

        // If project name is explicitly set in kinetics.toml, return it right away
        if !config.project.name.is_empty() {
            return Ok(config);
        }

        config.project.name = Self::cargo_toml_name(path.as_path())?;
        Ok(config)
    }

    /// Reads Cargo.toml in a given directory and returns the name
    fn cargo_toml_name(path: &Path) -> eyre::Result<String> {
        let cargo_toml_path = path.join("Cargo.toml");

        let cargo_toml_string = fs::read_to_string(&cargo_toml_path).wrap_err(Error::new(
            &format!("Failed to read {cargo_toml_path:?}"),
            None,
        ))?;

        let cargo_toml: toml::Value =
            cargo_toml_string
                .parse::<toml::Value>()
                .wrap_err(Error::new(
                    &format!("Failed to parse TOML in {cargo_toml_path:?}"),
                    None,
                ))?;

        let name = cargo_toml
            .get("package")
            .and_then(|pkg| pkg.get("name"))
            .and_then(|name| name.as_str())
            .wrap_err(Error::new(
                &format!("No crate name property in {cargo_toml_path:?}"),
                Some("Cargo.toml is invalid, or you are in a wrong dir."),
            ))?
            .to_string();

        Ok(name)
    }
}

impl From<ConfigFile> for Project {
    fn from(cfg: ConfigFile) -> Self {
        Project {
            path: cfg.path,
            name: cfg.project.name,
            url: "".to_string(), // Unknown at this point, will be populated by the API
            kvdb: cfg.kvdb,
        }
    }
}
