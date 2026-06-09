mod cache;
mod config_file;
mod filehash;
mod parse;

/// Runtime templates for different workloads
mod templates;
mod workspace;

use crate::api::client::Client;
use crate::api::projects::{Kvdb, ProjectInfo};
use crate::api::request::Validate;
use crate::api::stack;
use crate::config::deploy::DeployConfig;
use crate::envs::Envs;
use crate::error::Error;
use crate::function::Function;
use crate::project::workspace::Workspace;
use crate::secrets::Secrets;
use cache::Cache;
use config_file::ConfigFile;
use eyre::WrapErr;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use toml_edit::{value, DocumentMut, Table};

/// Managing user's project
///
/// Used for handling configuration and calling relevant APIs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    #[serde(skip)]
    pub path: PathBuf,

    #[serde(skip)]
    pub workspace: Workspace,

    /// Project name (used as a prefix for all resources)
    pub name: String,

    /// URL of the project, e.g. https://project-name.kinetics.app
    pub url: String,

    /// KVDBs to be created
    pub kvdb: Vec<Kvdb>,

    pub observability: Option<Observability>,

    /// Custom domain name for the project
    pub domain_name: Option<String>,
    /// Org is optional and set in kinetics.toml
    pub org: Option<String>,
}

/// Project's settings for observability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observability {
    pub dd_api_key: String,
}

impl Project {
    fn new(path: PathBuf, name: String) -> Self {
        let workspace = Workspace::from_path(&path).ok().unwrap_or_default();

        Self {
            path,
            workspace,
            name,
            url: String::new(),
            kvdb: Vec::new(),
            observability: None,
            domain_name: None,
            org: None,
        }
    }

    fn set_observability(mut self, dd_api_key: String) -> Self {
        self.observability = Some(Observability { dd_api_key });
        self
    }

    fn set_kvdb(mut self, kvdb: Vec<Kvdb>) -> Self {
        self.kvdb = kvdb;
        self
    }

    pub fn with_org(mut self, org: Option<&str>) -> Self {
        self.org = org.map(|o| o.to_string());
        self
    }

    /// Creates a new project instance by reading `kinetics.toml` from a given file `path`
    ///
    /// Returns default config if kinetics.toml does not exist. In that case the name will be taken
    /// from the ` Cargo.toml ` file in the same path
    pub fn from_path(path: PathBuf) -> eyre::Result<Self> {
        let cfg = ConfigFile::from_path(path)?;
        // Convert the config file to a Project instance with existing trait
        cfg.try_into()
    }

    /// Get project by name, with automatic cache management.
    ///
    /// Returns an error if the API request fails or if there are filesystem issues
    /// with reading/writing the cache.
    pub async fn fetch_one(name: &str, org: Option<&str>) -> eyre::Result<Self> {
        let cache = Cache::new(org).await?;

        cache
            .get(name)
            .wrap_err("Failed to load project information")
    }

    /// Get a list of projects created by user
    ///
    /// Returns an error if the API request fails or if there are filesystem issues
    /// with reading/writing the cache.
    pub async fn fetch_all(org: Option<&str>) -> eyre::Result<Vec<Self>> {
        Cache::new(org)
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
                project: self.clone(),
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
        version_message: Option<String>,
    ) -> eyre::Result<bool> {
        let client = Client::new(deploy_config.is_some()).await?;
        let secrets = Secrets::load();

        if let Some(config) = deploy_config {
            return config.deploy(self, secrets, functions).await;
        }

        let request = stack::deploy::Request {
            is_hotswap,
            secrets,
            version_message,
            functions: functions
                .iter()
                .map(|f| f.into())
                .collect::<Vec<stack::deploy::FunctionRequest>>(),
            project: self.clone(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))).into());
        }

        log::debug!(
            "Sending request to deploy:\n{}",
            serde_json::to_string_pretty(&request)?
        );

        let result = client
            .post("/stack/deploy")
            .json(&request)
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
        let client = Client::new(false).await?;

        let result = client
            .post("/stack/status")
            .json(&stack::status::Request {
                name: self.name.to_owned(),
                project: self.clone(),
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

    /// Make sure URL is properly formatted
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

    /// Write the current project config to config file
    pub fn write_config(&self) -> eyre::Result<()> {
        let config_path = self.path.join("kinetics.toml");

        let mut doc = if config_path.exists() {
            fs::read_to_string(&config_path)
                .wrap_err("Failed to read kinetics.toml")?
                .parse::<DocumentMut>()
                .wrap_err("Failed to parse kinetics.toml")?
        } else {
            DocumentMut::new()
        };

        // Ensure [project] table exists
        if doc.get("project").is_none() {
            doc["project"] = toml_edit::Item::Table(Table::new());
        }

        // Always write the name
        if !self.name.is_empty() {
            doc["project"]["name"] = value(&self.name);
        }

        // Set or remove org
        match &self.org {
            Some(org) => {
                doc["project"]["org"] = value(org);
            }
            None => {
                if let Some(project) = doc.get_mut("project").and_then(|p| p.as_table_mut()) {
                    project.remove("org");
                }
            }
        }

        match &self.domain_name {
            Some(domain) => {
                doc["domain"] = value(domain);
            }
            None => {
                doc.remove("domain");
            }
        }

        fs::write(&config_path, doc.to_string()).wrap_err("Failed to write kinetics.toml")?;
        Ok(())
    }
}

impl From<ProjectInfo> for Project {
    fn from(value: ProjectInfo) -> Self {
        Self {
            path: PathBuf::new(),
            workspace: Workspace::default(),
            name: value.name,
            url: value.url,
            kvdb: value.kvdb,
            org: value.org,
            observability: None,
            domain_name: None,
        }
    }
}
