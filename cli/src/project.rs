use crate::client::Client;
use crate::config::build_config;
use crate::error::Error;
use chrono::{DateTime, Duration, Utc};
use eyre::{ContextCompat, WrapErr};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
const CACHE_EXPIRES_IN: Duration = Duration::minutes(10);

#[derive(Debug, Serialize, Deserialize)]
struct ProjectsResponse {
    projects: Vec<Project>,
}

/// The structure of entire cache file
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
            .wrap_err(format!("Failed to write cache"))?;

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
                .wrap_err(format!("Failed to clear the projects cache"))?;
        }

        Ok(())
    }

    /// Get the static cache path for storing project information.
    fn path() -> eyre::Result<PathBuf> {
        Ok(PathBuf::from(build_config()?.kinetics_path).join(".projects"))
    }

    async fn load() -> eyre::Result<Self> {
        let response = Client::new(false).await?
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

/// Project (crate) info
///
/// Such as base URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub url: String,
}

impl Project {
    /// Get project by name, with automatic cache management.
    ///
    /// Returns an error if the API request fails or if there are filesystem issues
    /// with reading/writing the cache.
    pub async fn one(name: &str) -> eyre::Result<Self> {
        let cache = Cache::new().await?;

        cache
            .get(name)
            .wrap_err("Failed to load project information")
            .map(Into::into)
    }

    /// Get a list of projects created by user
    ///
    /// Returns an error if the API request fails or if there are filesystem issues
    /// with reading/writing the cache.
    pub async fn all() -> eyre::Result<Vec<Self>> {
        Cache::new()
            .await
            .map(|cache| cache.projects.values().map(|p| p.clone()).collect())
    }

    pub fn clear_cache() -> eyre::Result<()> {
        Cache::clear()
    }
}
