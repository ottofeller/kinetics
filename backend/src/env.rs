use eyre::ContextCompat;
use std::collections::HashMap;

/// Return the env or throw an error
pub fn env(name: &str) -> eyre::Result<String> {
    std::env::vars()
        .collect::<HashMap<_, _>>()
        .get(name)
        .wrap_err(format!("{} is missing", name))
        .map(|s| s.to_owned())
}
