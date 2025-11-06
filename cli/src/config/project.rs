use eyre::Context;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Project is the project global configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Project {
    /// Project name (used as a prefix for all resources)
    pub name: String,

    /// KVDBs to be created
    pub kvdb: Vec<Kvdb>,

    /// Enables or disables SQL DB for the project
    pub sqldb: Sqldb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kvdb {
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sqldb {
    pub is_enabled: bool,
}

impl Project {
    pub fn from_path(name: &str, path: PathBuf) -> eyre::Result<Self> {
        let config_toml_path = path.join("kinetics.toml");

        if let Ok(toml_string) = std::fs::read_to_string(&config_toml_path) {
            let config: FileConfig =
                toml::from_str(&toml_string).wrap_err("Failed to parse kinetics.toml")?;

            // Convert config to Project struct
            let mut project: Project = config.into();

            // If name is not set, use the project name from Cargo.toml
            if project.name.is_empty() {
                project.name = name.to_string();
            }

            Ok(project)
        } else {
            // Just use a default config if kinetics.toml is not found.
            Ok(Project {
                name: name.to_string(),
                ..Default::default()
            })
        }
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

    /// [sqldb]
    /// enabled = true
    #[serde(default)]
    pub sqldb: SqldbSection,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ProjectSection {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SqldbSection {
    pub enabled: bool,
}

impl From<FileConfig> for Project {
    fn from(cfg: FileConfig) -> Self {
        Project {
            name: cfg.project.name.unwrap_or_default(),
            kvdb: cfg.kvdb,
            sqldb: Sqldb {
                is_enabled: cfg.sqldb.enabled,
            },
        }
    }
}
