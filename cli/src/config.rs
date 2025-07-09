use crate::error::Error;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug)]
pub(crate) struct Config<'a> {
    pub(crate) api_base: &'a str,
    pub(crate) domain: &'a str,
    pub(crate) build_path: &'a str,
    pub(crate) credentials_path: &'a str,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub(crate) fn build_config() -> Result<&'static Config<'static>, Error> {
    let home_dir = std::env::var("HOME").map_err(|_| {
        log::error!("Failed to get $HOME");

        Error::new(
            "$HOME is missing",
            Some("Your shell might not be supported."),
        )
    })?;

    Ok(CONFIG.get_or_init(|| {
        let api_base =
            option_env!("KINETICS_API_BASE").unwrap_or("https://backend.usekinetics.com/");

        let build_path_raw = Path::new(&home_dir).join(".kinetics");

        // Create a static string to avoid referencing temporary value
        let credentials_path = Box::leak(
            build_path_raw
                .join(".credentials")
                .display()
                .to_string()
                .into_boxed_str(),
        );

        let build_path = Box::leak(
            Path::new(&home_dir)
                .join(".kinetics")
                .display()
                .to_string()
                .into_boxed_str(),
        );

        Config {
            api_base,
            build_path,
            credentials_path,
            domain: "usekinetics.com",
        }
    }))
}

pub fn api_url(path: &str) -> String {
    format!("{}{}", build_config().unwrap().api_base, path)
}
