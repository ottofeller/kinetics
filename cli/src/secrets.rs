use rust_dotenv::dotenv::DotEnv;
use std::collections::HashMap;
use std::path::Path;

const FILENAME: &str = ".env.secrets";
const PREFIX: &str = "KINETICS_SECRET_";

pub struct Secrets;

impl Secrets {
    /// Read secrets from the project's .env file or env vars if file not found.
    pub fn load(project_dir: &Path) -> HashMap<String, String> {
        let path = project_dir.join(FILENAME);

        if !path.exists() {
            log::warn!(
                "No {FILENAME} file found in {}. Search for {PREFIX} prefixed environment variables.",
                project_dir.display()
            );
            return std::env::vars()
                .filter_map(|(prefixed_name, value)| {
                    if prefixed_name.starts_with(PREFIX) && prefixed_name != PREFIX {
                        prefixed_name
                            .strip_prefix(PREFIX)
                            .map(|name| (name.to_owned(), value))
                    } else {
                        None
                    }
                })
                .collect();
        }

        DotEnv::load_env(&path.to_string_lossy()).unwrap_or_default()
    }
}
