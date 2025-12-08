use rust_dotenv::dotenv::DotEnv;
use std::collections::HashMap;

const FILENAME: &str = ".env.secrets";
const PREFIX: &str = "KINETICS_SECRET_";

pub struct Secrets;

impl Secrets {
    /// Read secrets from the .env file or env vars if file not found.
    pub fn load() -> HashMap<String, String> {
        if !std::path::Path::new(FILENAME).exists() {
            log::warn!(
                "No .env.secrets file found. Search for {PREFIX} prefixed environment variables."
            );
            return std::env::vars()
                .filter(|(name, _)| name.starts_with(PREFIX) && name != PREFIX)
                .collect();
        }

        DotEnv::load_env(FILENAME).unwrap_or_default()
    }
}
