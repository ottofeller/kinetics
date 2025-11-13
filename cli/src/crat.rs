use crate::client::Client;
use crate::commands::deploy::DeployConfig;
use crate::error::Error;
use crate::function::{Function, Role};
use crate::project::Project;
use crate::secret::Secret;
use eyre::{Ok, WrapErr};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Crate {
    /// Path to the crate's root directory
    pub path: PathBuf,

    /// Current project, will reaplce the Crate struct completely in the next iteration
    pub project: Project,
}

#[derive(serde::Deserialize, Debug)]
pub struct StatusResponseBody {
    pub status: String,
    pub errors: Option<Vec<String>>,
}

impl Crate {
    pub fn new(path: PathBuf) -> eyre::Result<Self> {
        Ok(Crate {
            path: path.clone(),
            project: Project::from_path(path)?,
        })
    }

    /// Return crate info from Cargo.toml
    pub fn from_current_dir() -> eyre::Result<Self> {
        Self::new(std::env::current_dir().wrap_err("Failed to get current dir")?)
    }

    /// Deploy all assets using CFN template
    /// The boolean returned indicates whether the stack was updated.
    pub async fn deploy(
        &self,
        functions: &[Function],
        deploy_config: Option<&dyn DeployConfig>,
    ) -> eyre::Result<bool> {
        let client = Client::new(deploy_config.is_some()).await?;

        let secrets = HashMap::from_iter(
            Secret::from_dotenv()
                .iter()
                .map(|s| (s.name.clone(), s.value())),
        );

        if let Some(config) = deploy_config {
            return config.deploy(&self.project, secrets, functions).await;
        }

        let body = DeployRequest {
            secrets,
            functions: functions
                .iter()
                .map(|f| f.into())
                .collect::<Vec<FunctionRequest>>(),
            project: self.project.clone(),
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
        log::info!("got status from /stack/deploy: {}", status);
        log::info!("got response from /stack/deploy: {}", result.text().await?);

        match status {
            StatusCode::OK => Ok(true),
            StatusCode::NOT_MODIFIED => Ok(false),
            _ => Err(Error::new(
                "Deployment request failed",
                Some("Try again in a few seconds."),
            )
            .into()),
        }
    }

    pub async fn status(&self) -> eyre::Result<StatusResponseBody> {
        Self::status_by_name(&self.project.name).await
    }

    pub async fn status_by_name(name: &str) -> eyre::Result<StatusResponseBody> {
        let client = Client::new(false).await?;

        #[derive(serde::Serialize, Debug)]
        pub struct JsonBody {
            pub name: String,
        }

        let result = client
            .post("/stack/status")
            .json(&JsonBody {
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

        let status: StatusResponseBody =
            serde_json::from_str(&text).wrap_err("Failed to parse response")?;

        Ok(status)
    }
}

/// A structure representing a deployment request which contains configuration, secrets, and functions to be deployed.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DeployRequest {
    pub project: Project,
    pub secrets: HashMap<String, String>,
    pub functions: Vec<FunctionRequest>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FunctionRequest {
    pub is_deploying: bool,
    pub name: String,
    pub role: Role,
}

impl From<&Function> for FunctionRequest {
    fn from(f: &Function) -> Self {
        Self {
            name: f.name.clone(),
            is_deploying: f.is_deploying,
            role: f.role.clone(),
        }
    }
}
