use rust_dotenv::dotenv::DotEnv;
use std::collections::HashMap;
use std::path::Path;

const FILENAME: &str = ".env.secrets";
const PREFIX: &str = "KINETICS_SECRET_";

pub struct Secrets;

impl Secrets {
    /// Read secrets from the `.env.secrets` files found in `dirs`.
    ///
    /// Files are merged in the given order,
    /// so later entries take priority on key conflicts.
    /// Returns `None` when no file is found.
    pub fn from_files(dirs: &[&Path]) -> Option<HashMap<String, String>> {
        let files: Vec<_> = dirs
            .iter()
            .map(|dir| dir.join(FILENAME))
            .filter(|path| path.exists())
            .collect();

        if files.is_empty() {
            log::warn!(
                "No {FILENAME} file found in {}.",
                dirs.iter()
                    .map(|d| d.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            return None;
        }

        Some(
            files
                .iter()
                .flat_map(|path| DotEnv::load_env(&path.to_string_lossy()).unwrap_or_default())
                .collect(),
        )
    }

    /// Collect `KINETICS_SECRET_` prefixed environment variables as secrets.
    pub fn from_env() -> HashMap<String, String> {
        log::debug!("Search for {PREFIX} prefixed environment variables.");
        std::env::vars()
            .filter_map(|(prefixed_name, value)| {
                if prefixed_name.starts_with(PREFIX) && prefixed_name != PREFIX {
                    prefixed_name
                        .strip_prefix(PREFIX)
                        .map(|name| (name.to_owned(), value))
                } else {
                    None
                }
            })
            .collect()
    }
}
