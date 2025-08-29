use crate::client::Client;
use crate::config::build_config;
use crate::crat::Crate;
use crate::error::Error;
use chrono::{DateTime, Duration, Utc};
use eyre::WrapErr;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Overall project's info
///
/// Stored in a sort of a cache on file system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    pub url: String,
    pub last_updated: DateTime<Utc>,
}

/// The structure of entire cache file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cache {
    projects: HashMap<String, Info>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BaseUrlRequest {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BaseUrlResponse {
    url: String,
}

static CACHE_EXPIRES_IN: Duration = Duration::minutes(10);

/// Project (crate) info
///
/// Such as base URL.
pub struct Project {
    crat: Crate,
}

impl Project {
    /// Create a new Project instance for the given crate.
    pub fn new(crat: Crate) -> Self {
        Project { crat }
    }

    /// Get the static cache path for storing project information.
    fn cache_path() -> eyre::Result<PathBuf> {
        Ok(PathBuf::from(build_config()?.build_path).join(".projects"))
    }

    /// Get project base URL, with automatic cache management.
    ///
    /// Returns an error if the API request fails or if there are filesystem issues
    /// with reading/writing the cache.
    pub async fn base_url(&self) -> eyre::Result<String> {
        let cache = self.load_cache().await?;
        let project_name = &self.crat.name;

        if let Some(project_info) = cache.projects.get(project_name) {
            return Ok(project_info.url.clone());
        }

        // Should not happen since load_cache handles fetching fresh data
        Err(eyre::eyre!("Failed to load project information"))
    }

    /// Load the project cache from disk with automatic refresh logic
    async fn load_cache(&self) -> eyre::Result<Cache> {
        let cache_path = Self::cache_path()?;
        let project_name = &self.crat.name;

        let mut cache = if !cache_path.exists() {
            // Create the cache directory if it doesn't exist
            if let Some(parent) = cache_path.parent() {
                fs::create_dir_all(parent)
                    .wrap_err(format!("Failed to create cache directory {:?}", parent))?;
            }

            Cache {
                projects: HashMap::new(),
            }
        } else {
            let cache_content = fs::read_to_string(&cache_path)
                .wrap_err(format!("Failed to read cache file {:?}", cache_path))?;

            serde_json::from_str(&cache_content).unwrap_or_else(|_| Cache {
                projects: HashMap::new(),
            })
        };

        // Check if we need to refresh the cache for this project
        let is_expired = if let Some(project_info) = cache.projects.get(project_name) {
            let now = Utc::now();
            let cache_age = now - project_info.last_updated;
            cache_age >= CACHE_EXPIRES_IN
        } else {
            true // No cached data exists
        };

        if is_expired {
            // Fetch fresh data from API
            let client = Client::new(false)?;

            let response = client
                .request::<BaseUrlRequest, BaseUrlResponse>(
                    "/project/url",
                    BaseUrlRequest {
                        name: project_name.clone(),
                    },
                )
                .await
                .wrap_err(Error::new(
                    "Failed to fetch project information",
                    Some("Try again in a few seconds."),
                ))?;

            println!("RESPONSE: {response:?}");

            // Update cache with fresh data
            let project_info = Info {
                url: response.url,
                last_updated: Utc::now(),
            };

            cache.projects.insert(project_name.clone(), project_info);

            // Save cache to the file
            let cache_path = Self::cache_path()?;

            let cache_json = serde_json::to_string_pretty(&cache)
                .wrap_err("Failed to serialize project cache")?;

            fs::write(&cache_path, cache_json)
                .wrap_err(format!("Failed to write cache file {:?}", cache_path))?;
        }

        Ok(cache)
    }

    pub fn clear_cache() -> eyre::Result<()> {
        let cache_path = Self::cache_path()?;

        if cache_path.exists() {
            fs::remove_file(&cache_path)
                .inspect_err(|e| log::error!("Failed to remove cache file {cache_path:?}: {e:?}"))
                .wrap_err(format!("Failed to clear the projects cache"))?;
        }

        Ok(())
    }
}
