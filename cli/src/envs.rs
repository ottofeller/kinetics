use rust_dotenv::dotenv::DotEnv;
use std::collections::HashMap;
pub struct Envs;

/// Env variables defined in outer files
impl Envs {
    /// Read secrets from the .env file or env vars if file not found.
    pub fn load() -> HashMap<String, String> {
        if !std::path::Path::new(".env").exists() {
            log::debug!("No .env file found");
            return HashMap::new()
        }

        DotEnv::new("").all_vars().to_owned()
    }
}
