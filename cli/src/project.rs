mod cache;
mod config_file;
mod filehash;
mod parse;

/// Runtime templates for different workloads
mod templates;

use crate::api::client::Client;
use crate::api::projects::Kvdb;
use crate::api::stack;
use crate::config::deploy::DeployConfig;
use crate::envs::Envs;
use crate::error::Error;
use crate::function::Function;
use crate::secrets::Secrets;
use cache::Cache;
use config_file::ConfigFile;
use eyre::WrapErr;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Managing user's project
///
/// Used for handling configuration and calling relevant APIs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    #[serde(skip)]
    pub path: PathBuf,

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
        Ok(ConfigFile::from_path(path)?.into())
    }

    /// Creates a new project instance from the current directory
    pub fn from_current_dir() -> eyre::Result<Self> {
        Self::from_path(std::env::current_dir().wrap_err("Failed to get current dir")?)
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
            .map(|cache| cache.projects.into_values().collect())
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
            .json(&stack::destroy::Request {
                project_name: self.name.to_owned(),
            })
            .send()
            .await?;

        Ok(())
    }

    /// Deploy all assets using CFN template
    /// The boolean returned indicates whether the stack was updated.
    pub async fn deploy(
        &self,
        functions: &[Function],
        is_hotswap: bool,
        deploy_config: Option<&dyn DeployConfig>,
    ) -> eyre::Result<bool> {
        let client = Client::new(deploy_config.is_some()).await?;
        let secrets = Secrets::load();

        if let Some(config) = deploy_config {
            return config.deploy(self, secrets, functions).await;
        }

        let body = stack::deploy::Request {
            is_hotswap,
            secrets,
            functions: functions
                .iter()
                .map(|f| f.into())
                .collect::<Vec<stack::deploy::FunctionRequest>>(),
            project: self.clone(),
        };

        log::debug!(
            "Sending request to deploy:\n{}",
            serde_json::to_string_pretty(&body)?
        );

        let result = client
            .post("/stack/deploy")
            .json(&body)
            .send()
            .await
            .inspect_err(|err| log::error!("Error while requesting deploy: {err:?}"))
            .wrap_err(Error::new(
                "Network request failed",
                Some("Try again in a few seconds."),
            ))?;

        let status = result.status();
        log::info!("got status from /stack/deploy: {status}");
        log::info!("got response from /stack/deploy: {}", result.text().await?);

        match status {
            StatusCode::OK => eyre::Ok(true),
            StatusCode::NOT_MODIFIED => eyre::Ok(false),
            _ => Err(Error::new(
                "Deployment request failed",
                Some("Try again in a few seconds."),
            )
            .into()),
        }
    }

    pub async fn status(&self) -> eyre::Result<stack::status::Response> {
        Self::status_by_name(&self.name).await
    }

    pub async fn status_by_name(name: &str) -> eyre::Result<stack::status::Response> {
        let client = Client::new(false).await?;

        let result = client
            .post("/stack/status")
            .json(&stack::status::Request {
                name: name.to_owned(),
            })
            .send()
            .await
            .wrap_err(Error::new(
                "Network request failed",
                Some("Try again in a few seconds."),
            ))?;

        let status = result.status();
        let text = result.text().await?;
        log::debug!("Got response from /stack/status:\n{status}\n{text}");

        if status != StatusCode::OK {
            return Err(
                Error::new("Status request failed", Some("Try again in a few seconds.")).into(),
            );
        }

        serde_json::from_str(&text).wrap_err("Failed to parse response")
    }

    /// Make sure URL is properly foramtted
    ///
    /// For example API Gateway are case sensitive.
    pub fn url(&self) -> String {
        self.url.to_lowercase()
    }

    /// Globally applied env vars sourced from .env file
    ///
    /// No need to store it in Project props, it's not going to be loaded frequently
    pub fn environment(&self) -> HashMap<String, String> {
        Envs::load()
    }
}
