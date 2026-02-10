use crate::api::client::Client;
use crate::api::projects;
use crate::config::build_config;
use crate::error::Error;
use crate::project::Project;
use chrono::Duration;
use chrono::{DateTime, Utc};
use eyre::{ContextCompat, WrapErr};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
const CACHE_EXPIRES_IN: Duration = Duration::minutes(10);

/// The structure of entire cache file
///
/// The cache is stored in a file, and gets refreshed automatically when it expires
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Cache {
    pub(super) projects: HashMap<String, Project>,
    last_updated: DateTime<Utc>,
}

impl Cache {
    /// Load the project cache from disk with automatic refresh logic
    pub(super) async fn new() -> eyre::Result<Self> {
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
    pub(super) fn get(&self, project_name: &str) -> eyre::Result<Project> {
        self.projects
            .get(project_name)
            .wrap_err("Project not found")
            .cloned()
    }

    pub(super) fn clear() -> eyre::Result<()> {
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
            .request::<(), projects::Response>("/projects", ())
            .await
            .wrap_err(Error::new(
                "Failed to fetch project information",
                Some("Try again in a few seconds."),
            ))?;

        let projects = HashMap::from_iter(
            response
                .projects
                .into_iter()
                .map(|project| (project.name.clone(), project.into())),
        );

        Ok(Self {
            projects,
            last_updated: Utc::now(),
        })
    }
}
