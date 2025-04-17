use crate::client::Client;
use crate::config;
use crate::deploy::deploy_directly;
use crate::error::Error;
use crate::function::Function;
use crate::secret::Secret;
use common::stack::{deploy, status};
use eyre::{ContextCompat, Ok, WrapErr};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Crate {
    /// Path to the crate's root directory
    pub path: PathBuf,
    pub name: String,
    pub toml: toml::Value,
    pub toml_string: String,
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
            path,
            name: cargo_toml
                .get("package")
                .and_then(|pkg| pkg.get("name"))
                .and_then(|name| name.as_str())
                .wrap_err(Error::new(
                    &format!("No crate name property in {cargo_toml_path:?}"),
                    Some("Cargo.toml is invalid, or you are in a wrong dir."),
                ))?
                .to_string(),

            toml: cargo_toml,
            toml_string: cargo_toml_string,
        })
    }

    /// Return crate info from Cargo.toml
    pub fn from_current_dir() -> eyre::Result<Self> {
        Self::new(std::env::current_dir().wrap_err("Failed to get current dir")?)
    }

    /// Deploy all assets using CFN template
    pub async fn deploy(&self, functions: &[Function]) -> eyre::Result<()> {
        let client = Client::new()?;
        let mut secrets = HashMap::new();

        Secret::from_dotenv().iter().for_each(|s| {
            secrets.insert(s.name.clone(), s.value());
        });

        // Provision the template directly if the flag is set
        if config::DIRECT_DEPLOY_ENABLED {
            return deploy_directly(self.toml_string.clone(), secrets, functions).await;
        }

        let result = client
            .post("/stack/deploy")
            .json(&serde_json::json!(deploy::JsonBody {
                crat: deploy::BodyCrate {
                    toml: self.toml_string.clone(),
                },
                functions: functions
                    .iter()
                    .map(|f| {
                        deploy::BodyFunction {
                            name: f.name().unwrap().to_string(),
                            s3key_encrypted: f.s3key_encrypted().unwrap(),
                            toml: f.toml_string().unwrap(),
                        }
                    })
                    .collect(),
                secrets: secrets.clone(),
            }))
            .send()
            .await
            .wrap_err(Error::new(
                "Network request failed",
                Some("Try again in a few seconds."),
            ))?;

        if result.status() != StatusCode::OK {
            return Err(Error::new(
                "Deployment request failed",
                Some("Try again in a few seconds."),
            )
            .into());
        }

        Ok(())
    }

    pub async fn status(&self) -> eyre::Result<status::ResponseBody> {
        let client = Client::new()?;

        let result = client
            .post("/stack/status")
            .json(&serde_json::json!(status::JsonBody {
                name: self.name.clone()
            }))
            .send()
            .await
            .wrap_err(Error::new(
                "Network request failed",
                Some("Try again in a few seconds."),
            ))?;

        if result.status() != StatusCode::OK {
            return Err(
                Error::new("Status request failed", Some("Try again in a few seconds.")).into(),
            );
        }

        let status: status::ResponseBody =
            result.json().await.wrap_err("Failed to parse response")?;

        Ok(status)
    }
}
