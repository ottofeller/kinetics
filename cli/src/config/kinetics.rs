use eyre::Context;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kvdb {
    name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KineticsConfig {
    kvdb: Vec<Kvdb>,
}

impl KineticsConfig {
    pub fn from_path(path: PathBuf) -> eyre::Result<Self> {
        let config_toml_path = path.join("kinetics.toml");

        if let Ok(toml_string) = std::fs::read_to_string(&config_toml_path) {
            let config = toml::from_str(&toml_string).wrap_err("Failed to parse kinetics.toml")?;
            Ok(config)
        } else {
            // Just use a default config if kinetics.toml is not found.
            Ok(KineticsConfig::default())
        }
    }
}
