use rust_dotenv::dotenv::DotEnv;

const FILENAME: &str = ".env.secrets";
const PREFIX: &str = "KINETICS_SECRET_";

pub struct Secret {
    pub name: String,
    value: String,
}

impl Secret {
    /// Read secrets from the .env file or env vars if file not found.
    pub fn load() -> Vec<Self> {
        if !std::path::Path::new(FILENAME).exists() {
            log::warn!(
                "No .env.secrets file found. Search for {PREFIX} prefixed environment variables."
            );
            return std::env::vars()
                .filter(|(name, _)| name.starts_with(PREFIX) && name != PREFIX)
                .map(|(name, value)| Secret {
                    name: name.replacen(PREFIX, "", 1),
                    value,
                })
                .collect();
        }

        DotEnv::load_env(FILENAME)
            .unwrap_or_default()
            .into_iter()
            .map(|(name, value)| Secret { name, value })
            .collect()
    }

    /// Create a key-value pair of a secret.
    /// If prefixed is true, the KINETICS_SECRET_ prefix is added to the name .
    pub fn into_tuple(self, prefixed: bool) -> (String, String) {
        (
            self.name,
            if prefixed {
                format!("{PREFIX}{}", self.value)
            } else {
                self.value
            },
        )
    }
}
