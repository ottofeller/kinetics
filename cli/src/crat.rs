use crate::client::Client;
use crate::commands::deploy::DeployConfig;
use crate::config::KineticsConfig;
use crate::error::Error;
use crate::function::Function;
use crate::secret::Secret;
use eyre::{ContextCompat, Ok, WrapErr};
use kinetics_parser::Role;
use reqwest::StatusCode;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Crate {
    /// Path to the crate's root directory
    pub path: PathBuf,

    /// Crate (project) name
    pub name: String,

    /// User-defined config
    pub config: KineticsConfig,
}

#[derive(serde::Deserialize, Debug)]
pub struct StatusResponseBody {
    pub status: String,
    pub errors: Option<Vec<String>>,
}

impl Crate {
    pub fn new(path: PathBuf) -> eyre::Result<Self> {
        let cargo_toml_path = path.join("Cargo.toml");

        let cargo_toml_string = std::fs::read_to_string(&cargo_toml_path).wrap_err(Error::new(
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

        Ok(Crate {
            path: path.clone(),

            name: cargo_toml
                .get("package")
                .and_then(|pkg| pkg.get("name"))
                .and_then(|name| name.as_str())
                .wrap_err(Error::new(
                    &format!("No crate name property in {cargo_toml_path:?}"),
                    Some("Cargo.toml is invalid, or you are in a wrong dir."),
                ))?
                .to_string(),

            config: KineticsConfig::from_path(path)?,
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
            return config.deploy(&self.config, secrets, functions).await;
        }

        let body = DeployRequest {
            secrets,
            functions: functions
                .iter()
                .map(|f| BodyFunction::try_from_function(f))
                .collect::<eyre::Result<Vec<BodyFunction>>>()?,
            config: self.config.clone(),
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
        Self::status_by_name(&self.name).await
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
#[derive(Debug, serde::Serialize)]
pub struct DeployRequest {
    pub config: KineticsConfig,
    pub secrets: HashMap<String, String>,
    pub functions: Vec<BodyFunction>,
}

#[derive(serde::Serialize, Debug)]
pub struct BodyFunction {
    is_deploying: bool,
    name: String,
    environment: HashMap<String, String>,

    // FIXME Config and role aren't the same thing consider to use a different name
    config: Role,
}

impl BodyFunction {
    pub fn try_from_function(f: &Function) -> eyre::Result<Self> {
        Ok(Self {
            name: f.name.clone(),
            environment: f.environment()?,
            is_deploying: f.is_deploying,
            config: f.role.clone(),
        })
    }
}
