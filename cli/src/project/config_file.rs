use crate::api::projects::Kvdb;
use crate::error::Error;
use crate::project::Project;
use eyre::{ContextCompat, WrapErr};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// FileConfig is the structure of kinetics.toml
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ConfigFile {
    #[serde(default)]
    project: ProjectSection,

    #[serde(default)]
    observability: Option<ObservabilitySection>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    kvdb: Vec<Kvdb>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    domain: Option<DomainSection>,

    #[serde(skip)]
    path: PathBuf,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct ProjectSection {
    name: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct ObservabilitySection {
    dd_api_key_env: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct DomainSection {
    name: String,
}

/// FileConfig is the structure of kinetics.toml
impl ConfigFile {
    /// Reads a `FileConfig` instance from a given directory path
    ///
    /// This function looks for a `kinetics.toml` file in the specified directory.
    /// If the `kinetics.toml` file is not present or cannot be read, it returns a default
    /// configuration instead. Additionally, if the `kinetics.toml` file does not explicitly set
    /// the project name, the function will fallback to extracting the name from a `Cargo.toml`
    /// file in the same directory.
    pub fn from_path(path: PathBuf) -> eyre::Result<Self> {
        let config_toml_path = path.join("kinetics.toml");

        let Ok(toml_string) = fs::read_to_string(&config_toml_path) else {
            // Return default config if kinetics.toml is not found
            return Ok(Self {
                project: ProjectSection {
                    name: Self::cargo_toml_name(path.as_path())?,
                },
                path,
                ..Default::default()
            });
        };

        let result: Result<ConfigFile, toml::de::Error> = toml::from_str(&toml_string);

        let mut config = result.map_err(|error| eyre::eyre!(
            "Failed to parse kinetics.toml: {}\nCheck docs at https://github.com/ottofeller/kinetics",
            error.message().to_string()
        ))?;

        // Set the path to the directory containing kinetics.toml
        config.path = path.clone();

        if config
            .observability
            .as_ref()
            .is_some_and(|observability| observability.dd_api_key_env.is_empty())
        {
            return Err(eyre::eyre!(
                "When [observability] section presented in kinetics.toml
                both dd_api_key and service_name properties must be specified"
            ));
        }

        if config
            .domain
            .as_ref()
            .is_some_and(|domain| domain.name.is_empty())
        {
            return Err(eyre::eyre!(
                "When [domain] section presented in kinetics.toml
                name property must be specified"
            ));
        }

        // If project name is explicitly set in kinetics.toml, return it right away
        if !config.project.name.is_empty() {
            return Ok(config);
        }

        config.project.name = Self::cargo_toml_name(path.as_path())?;
        Ok(config)
    }

    /// Update the domain section with a new name or remove it if name is None
    pub fn set_domain_name(&mut self, name: Option<&str>) -> &mut Self {
        if let Some(name) = name {
            self.domain = Some(DomainSection {
                name: name.to_string(),
            });
        } else {
            self.domain = None;
        }
        self
    }

    pub fn save(&self) -> eyre::Result<()> {
        fs::write(
            self.path.join("kinetics.toml"),
            toml::to_string_pretty(&self)?,
        )?;
        Ok(())
    }

    /// Reads Cargo.toml in a given directory and returns the name
    fn cargo_toml_name(path: &Path) -> eyre::Result<String> {
        let cargo_toml_path = path.join("Cargo.toml");

        let cargo_toml_string = fs::read_to_string(&cargo_toml_path).wrap_err(Error::new(
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

        let name = cargo_toml
            .get("package")
            .and_then(|pkg| pkg.get("name"))
            .and_then(|name| name.as_str())
            .wrap_err(Error::new(
                &format!("No crate name property in {cargo_toml_path:?}"),
                Some("Cargo.toml is invalid, or you are in a wrong dir."),
            ))?
            .to_string();

        Ok(name)
    }
}

impl TryFrom<ConfigFile> for Project {
    type Error = eyre::Report;

    fn try_from(cfg: ConfigFile) -> eyre::Result<Self> {
        let mut project = Project::new(cfg.path, cfg.project.name).set_kvdb(cfg.kvdb);

        if let Some(observability) = cfg.observability {
            // Read DataDog API key from env, it's not safe to store it in kinetics config file
            let dd_api_key = std::env::var(&observability.dd_api_key_env).unwrap_or_default();

            project = project.set_observability(dd_api_key);
        }

        if let Some(domain) = cfg.domain {
            project = project.set_domain_name(domain.name);
        }

        Ok(project)
    }
}
