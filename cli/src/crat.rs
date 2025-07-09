use crate::client::Client;
use crate::deploy::DeployConfig;
use crate::error::Error;
use crate::function::Function;
use crate::secret::Secret;
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

    /// The name used in project URL
    pub fn escaped_name(&self) -> String {
        self.name.replace('_', "-")
    }

    /// Deploy all assets using CFN template
    pub async fn deploy(
        &self,
        functions: &[Function],
        deploy_config: Option<&dyn DeployConfig>,
    ) -> eyre::Result<()> {
        #[derive(serde::Deserialize, serde::Serialize, Debug)]
        pub struct BodyCrate {
            // Full Cargo.toml
            pub toml: String,
        }

        #[derive(serde::Serialize, Debug)]
        pub struct BodyFunction {
            pub name: String,

            // Full Cargo.toml
            pub toml: String,
        }

        impl TryFrom<&Function> for BodyFunction {
            fn try_from(f: &Function) -> Result<Self, Self::Error> {
                let mut manifest = f.crat.toml.clone();
                let function_meta = manifest
                    .get("package")
                    .wrap_err("No [package]")?
                    .get("metadata")
                    .wrap_err("No [metadata]")?
                    .get("kinetics")
                    .wrap_err("No [kinetics]")?
                    .get(&f.name)
                    .wrap_err(format!("No [{}]", f.name))?
                    .clone();
                manifest["package"]["metadata"]["kinetics"] = function_meta;

                Ok(Self {
                    name: f.name.clone(),
                    toml: toml::to_string(&manifest)?,
                })
            }

            type Error = eyre::ErrReport;
        }

        #[derive(serde::Serialize, Debug)]
        pub struct JsonBody {
            pub crat: BodyCrate,
            pub functions: Vec<BodyFunction>,
            pub secrets: HashMap<String, String>,
        }

        let client = Client::new(deploy_config.is_some())?;
        let secrets = HashMap::from_iter(
            Secret::from_dotenv()
                .iter()
                .map(|s| (s.name.clone(), s.value())),
        );

        if let Some(config) = deploy_config {
            return config
                .deploy(self.toml_string.clone(), secrets, functions)
                .await;
        }

        let result = client
            .post("/stack/deploy")
            .json(&serde_json::json!(JsonBody {
                crat: BodyCrate {
                    toml: self.toml_string.clone(),
                },
                functions: functions
                    .iter()
                    .map(BodyFunction::try_from)
                    .collect::<eyre::Result<Vec<BodyFunction>>>()?,
                secrets,
            }))
            .send()
            .await
            .inspect_err(|err| log::error!("{err:?}"))
            .wrap_err(Error::new(
                "Network request failed",
                Some("Try again in a few seconds."),
            ))?;

        let status = result.status();
        log::info!("got status from /stack/deploy: {}", status);
        log::info!("got response from /stack/deploy: {}", result.text().await?);

        if status != StatusCode::OK {
            return Err(Error::new(
                "Deployment request failed",
                Some("Try again in a few seconds."),
            )
            .into());
        }

        Ok(())
    }

    pub async fn status(&self) -> eyre::Result<StatusResponseBody> {
        let client = Client::new(false)?;

        #[derive(serde::Serialize, Debug)]
        pub struct JsonBody {
            pub name: String,
        }

        let result = client
            .post("/stack/status")
            .json(&serde_json::json!(JsonBody {
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

        let status: StatusResponseBody =
            result.json().await.wrap_err("Failed to parse response")?;

        Ok(status)
    }
}
