use eyre::Context;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Managing user's project
///
/// Used for calling relevant API's and handling configuration. Maps one2one from user's crate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Project {
    /// Project name (used as a prefix for all resources)
    pub name: String,

    /// KVDBs to be created
    pub kvdb: Vec<Kvdb>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kvdb {
    pub name: String,
}

impl Project {
    pub fn from_path(name: &str, path: PathBuf) -> eyre::Result<Self> {
        Ok(FileConfig::from_path(name, path)?.into())
    }
}

/// FileConfig is the structure of kinetics.toml
#[derive(Debug, Clone, Default, Deserialize)]
struct FileConfig {
    /// [project]
    /// name = "some-project"
    #[serde(default)]
    pub project: ProjectSection,

    /// [[kvdb]]
    /// name = "kvdb"
    #[serde(default)]
    pub kvdb: Vec<Kvdb>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ProjectSection {
    pub name: String,
}

impl FileConfig {
    fn from_path(name: &str, path: PathBuf) -> eyre::Result<Self> {
        let config_toml_path = path.join("kinetics.toml");

        if let Ok(toml_string) = std::fs::read_to_string(&config_toml_path) {
            let mut config: FileConfig =
                toml::from_str(&toml_string).wrap_err("Failed to parse kinetics.toml")?;

            // Set fallback project name if not explicitly set in kinetics.toml
            if config.project.name.is_empty() {
                config.project.name = name.to_string();
            }

            Ok(config)
        } else {
            // Return default config if kinetics.toml is not found
            Ok(FileConfig::default())
        }
    }
}

impl From<FileConfig> for Project {
    fn from(cfg: FileConfig) -> Self {
        Project {
            name: cfg.project.name,
            kvdb: cfg.kvdb,
        }
    }
}
