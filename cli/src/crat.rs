use crate::client::Client;
use crate::config::config as build_config;
use crate::function::Function;
use crate::secret::Secret;
use backend::stack::deploy::{self, BodyCrate};
use backend::stack::status;
use backend::template::Crate as BackendCrate;
use backend::template::Function as BackendFunction;
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
        let cargo_toml_string = std::fs::read_to_string(path.join("Cargo.toml"))
            .wrap_err("Failed to read Cargo.toml: {cargo_toml_path:?}")?;

        let cargo_toml: toml::Value = cargo_toml_string
            .parse::<toml::Value>()
            .wrap_err("Failed to parse Cargo.toml")?;

        Ok(Crate {
            path,
            name: cargo_toml
                .get("package")
                .and_then(|pkg| pkg.get("name"))
                .and_then(|name| name.as_str())
                .wrap_err("Failed to get crate name from Cargo.toml")?
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
    pub async fn deploy(&self, functions: &[Function], is_directly: &bool) -> eyre::Result<()> {
        let client = Client::new(is_directly).wrap_err("Failed to create client")?;
        let mut secrets = HashMap::new();

        Secret::from_dotenv()?.iter().for_each(|s| {
            secrets.insert(s.name.clone(), s.value());
        });

        // Provision the template directly if the flag is set
        if *is_directly {
            let build_config = build_config();
            let crat =
                BackendCrate::new(self.toml_string.clone()).wrap_err("Invalid crate toml")?;

            let secrets = secrets
                .iter()
                .map(|(k, v)| {
                    backend::template::Secret::new(k, v, &crat, build_config.username_escaped)
                })
                .collect::<Vec<backend::template::Secret>>();

            let template = backend::template::Template::new(
                &crat,
                functions
                    .iter()
                    .map(|f| {
                        BackendFunction::new(
                            &f.toml_string().unwrap(),
                            &f.s3key_encrypted.to_owned().unwrap(),
                            "",
                            false,
                        )
                        .unwrap()
                    })
                    .collect::<Vec<BackendFunction>>(),
                secrets.clone(),
                build_config.s3_bucket_name,
                build_config.username_escaped,
                build_config.username,
                build_config.cloud_front_domain,
            )
            .await?;

            for secret in secrets.iter() {
                secret.sync().await?;
            }

            template
                .provision()
                .await
                .wrap_err("Failed to provision template")?;

            return Ok(());
        }

        let result = client
            .post("/stack/deploy")
            .json(&serde_json::json!(deploy::JsonBody {
                crat: BodyCrate {
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
            .wrap_err("Deployment request failed")?;

        if result.status() != StatusCode::OK {
            return Err(eyre::eyre!("Deployment failed: {:?}", result));
        }

        Ok(())
    }

    pub async fn status(&self) -> eyre::Result<status::ResponseBody> {
        let client = Client::new(&false).wrap_err("Failed to create client")?;

        let result = client
            .post("/stack/status")
            .json(&serde_json::json!(status::JsonBody {
                name: self.name.clone()
            }))
            .send()
            .await
            .wrap_err("Status request failed")?;

        if result.status() != StatusCode::OK {
            return Err(eyre::eyre!("Deployment failed: {:?}", result));
        }

        let status: status::ResponseBody = result
            .json()
            .await
            .wrap_err("Failed to parse status response")?;

        Ok(status)
    }
}
