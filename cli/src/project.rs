use crate::client::Client;
use crate::config::build_config;
use crate::error::Error;
use chrono::{DateTime, Duration, Utc};
use eyre::{ContextCompat, WrapErr};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const CACHE_EXPIRES_IN: Duration = Duration::minutes(10);

/// Managing user's project
///
/// Used for handling configuration and calling relevant APIs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Project name (used as a prefix for all resources)
    pub name: String,

    /// URL of the project, e.g. https://project-name.kinetics.app
    pub url: String,

    /// KVDBs to be created
    pub kvdb: Vec<Kvdb>,
}

impl Project {
    /// Creates a new project instance by reading `kinetics.toml` from a given file `path`
    ///
    /// Returns default config if kinetics.toml does not exist. In that case the name will be taken
    /// from the ` Cargo.toml ` file in the same path
    pub fn from_path(path: PathBuf) -> eyre::Result<Self> {
        Ok(FileConfig::from_path(path)?.into())
    }

    /// Get project by name, with automatic cache management.
    ///
    /// Returns an error if the API request fails or if there are filesystem issues
    /// with reading/writing the cache.
    pub async fn fetch_one(name: &str) -> eyre::Result<Self> {
        let cache = Cache::new().await?;

        cache
            .get(name)
            .wrap_err("Failed to load project information")
    }

    /// Get a list of projects created by user
    ///
    /// Returns an error if the API request fails or if there are filesystem issues
    /// with reading/writing the cache.
    pub async fn fetch_all() -> eyre::Result<Vec<Self>> {
        Cache::new()
            .await
            .map(|cache| cache.projects.values().cloned().collect())
    }

    pub fn clear_cache() -> eyre::Result<()> {
        Cache::clear()
    }

    /// Destroy the project by sending a DELETE request to /projects/{name}
    pub async fn destroy(&self) -> eyre::Result<()> {
        Client::new(false)
            .await
            .wrap_err("Failed to create client")?
            .post("/stack/destroy")
            .json(&json!({"crate_name": self.name}))
            .send()
            .await?;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ProjectsResponse {
    projects: Vec<Project>,
}

/// The structure of entire cache file
///
/// The cache is stored in a file, and gets refreshed automatically when it expires
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cache {
    projects: HashMap<String, Project>,
    last_updated: DateTime<Utc>,
}

impl Cache {
    /// Load the project cache from disk with automatic refresh logic
    async fn new() -> eyre::Result<Self> {
        let cache_path = Self::path()?;

        let cache: Option<Self> = if !cache_path.exists() {
            // Create the cache directory if it doesn't exist
            if let Some(parent) = cache_path.parent() {
                fs::create_dir_all(parent)
                    .inspect_err(|e| {
                        log::error!("Failed to create cache directory {parent:?}: {e:?}")
                    })
                    .wrap_err("Failed to create project cache")?;
            }

            None
        } else {
            // Read existing cache
            let cache_content = fs::read_to_string(&cache_path)
                .inspect_err(|e| log::error!("Failed to read cache file {cache_path:?}: {e:?}"))
                .wrap_err("Failed to load project cache")?;

            match serde_json::from_str(&cache_content) {
                Ok::<Cache, _>(cache) if Utc::now() - cache.last_updated < CACHE_EXPIRES_IN => {
                    Some(cache)
                }
                // The cache will be updated
                _ => None,
            }
        };

        // Load projects and populate cache if failed to read from disk
        let cache = match cache {
            Some(x) => x,
            None => Self::load().await?,
        };

        // Save cache to the file
        let cache_json = serde_json::to_string_pretty(&cache)
            .inspect_err(|e| log::error!("Failed to serialize project cache: {e:?}"))
            .wrap_err("Failed to process cache")?;

        let cache_path = Self::path()?;

        fs::write(&cache_path, cache_json)
            .inspect_err(|e| log::error!("Failed to write cache file {cache_path:?}: {e:?}"))
            .wrap_err("Failed to write cache")?;

        Ok(cache)
    }

    /// Get a project from cache
    pub fn get(&self, project_name: &str) -> eyre::Result<Project> {
        self.projects
            .get(project_name)
            .wrap_err("Project not found")
            .cloned()
    }

    pub fn clear() -> eyre::Result<()> {
        let cache_path = Self::path()?;

        if cache_path.exists() {
            fs::remove_file(&cache_path)
                .inspect_err(|e| log::error!("Failed to remove cache file {cache_path:?}: {e:?}"))
                .wrap_err("Failed to clear the projects cache")?;
        }

        Ok(())
    }

    /// Get the static cache path for storing project information.
    fn path() -> eyre::Result<PathBuf> {
        Ok(PathBuf::from(build_config()?.kinetics_path).join(".projects"))
    }

    async fn load() -> eyre::Result<Self> {
        let response = Client::new(false)
            .await?
            .request::<(), ProjectsResponse>("/projects", ())
            .await
            .wrap_err(Error::new(
                "Failed to fetch project information",
                Some("Try again in a few seconds."),
            ))?;

        let projects = HashMap::from_iter(
            response
                .projects
                .into_iter()
                .map(|project| (project.name.clone(), project)),
        );

        Ok(Self {
            projects,
            last_updated: Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kvdb {
    pub name: String,
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

/// FileConfig is the structure of kinetics.toml
impl FileConfig {
    /// Reads a `FileConfig` instance from a given directory path
    ///
    /// This function looks for a `kinetics.toml` file in the specified directory.
    /// If the `kinetics.toml` file is not present or cannot be read, it returns a default
    /// configuration instead. Additionally, if the `kinetics.toml` file does not explicitly set
    /// the project name, the function will fallback to extracting the name from a `Cargo.toml`
    /// file in the same directory.
    fn from_path(path: PathBuf) -> eyre::Result<Self> {
        let config_toml_path = path.join("kinetics.toml");

        let Ok(toml_string) = fs::read_to_string(&config_toml_path) else {
            // Return default config if kinetics.toml is not found
            return Ok(Self {
                project: ProjectSection {
                    name: Self::cargo_toml_name(path)?,
                },
                ..Default::default()
            });
        };

        let mut config: FileConfig =
            toml::from_str(&toml_string).wrap_err("Failed to parse kinetics.toml")?;

        // If project name is explicitly set in kinetics.toml, return it right away
        if !config.project.name.is_empty() {
            return Ok(config);
        }

        config.project.name = Self::cargo_toml_name(path)?;
        Ok(config)
    }

    /// Reads Cargo.toml in a given directory and returns the name
    fn cargo_toml_name(path: PathBuf) -> eyre::Result<String> {
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

impl From<FileConfig> for Project {
    fn from(cfg: FileConfig) -> Self {
        Project {
            name: cfg.project.name,
            url: "".to_string(), // Unknown at this point, will be populated by the API
            kvdb: cfg.kvdb,
        }
    }
}
