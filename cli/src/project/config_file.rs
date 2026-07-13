use crate::error::Error;
use eyre::{ContextCompat, WrapErr};
use kinetics_api::projects::Kvdb;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// FileConfig is the structure of kinetics.toml
#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct ConfigFile {
    #[serde(default)]
    pub(super) project: ProjectSection,

    #[serde(default)]
    pub(super) observability: Option<ObservabilitySection>,

    #[serde(default)]
    pub(super) kvdb: Vec<Kvdb>,

    #[serde(default)]
    pub(super) domain: Option<String>,

    #[serde(skip)]
    pub(super) path: PathBuf,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct ProjectSection {
    pub(super) name: String,
    pub(super) org: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct ObservabilitySection {
    pub(super) dd_api_key_env: String,
}

/// FileConfig is the structure of kinetics.toml
impl ConfigFile {
    pub(super) fn path(dir: &Path) -> PathBuf {
        dir.join("kinetics.toml")
    }

    pub(super) fn exists(dir: &Path) -> bool {
        Self::path(dir).exists()
    }

    /// Reads a `FileConfig` instance from a given directory path
    ///
    /// This function looks for a `kinetics.toml` file in the specified directory.
    /// If the `kinetics.toml` file is not present or cannot be read, it returns a default
    /// configuration instead. Additionally, if the `kinetics.toml` file does not explicitly set
    /// the project name, the function will fallback to extracting the name from a `Cargo.toml`
    /// file in the same directory.
    pub(super) fn from_path(dir: &Path) -> eyre::Result<Self> {
        let Ok(toml_string) = fs::read_to_string(Self::path(dir)) else {
            // Return default config if kinetics.toml is not found
            return Ok(Self {
                project: ProjectSection {
                    name: Self::cargo_toml_name(dir)?,
                    org: None,
                },
                path: dir.to_owned(),
                ..Default::default()
            });
        };

        let result: Result<ConfigFile, toml::de::Error> = toml::from_str(&toml_string);

        let mut config = result.map_err(|error| eyre::eyre!(
            "Failed to parse kinetics.toml: {}\nCheck docs at https://github.com/ottofeller/kinetics",
            error.message().to_string()
        ))?;

        // Set the path to the directory containing kinetics.toml
        config.path = dir.to_path_buf();

        if let Some(observability) = config.observability.as_ref() {
            if observability.dd_api_key_env.is_empty() {
                return Err(eyre::eyre!(
                    "When [observability] section presented in kinetics.toml
                        both dd_api_key and service_name properties must be specified"
                ));
            }
        }

        // If project name is explicitly set in kinetics.toml, return it right away
        if !config.project.name.is_empty() {
            return Ok(config);
        }

        config.project.name = Self::cargo_toml_name(dir)?;
        Ok(config)
    }

    /// Reads Cargo.toml in a given directory and returns the name
    pub fn cargo_toml_name(path: &Path) -> eyre::Result<String> {
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
