use crate::error::Error;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug)]
pub(crate) struct BuildConfig<'a> {
    pub(crate) api_base: &'a str,
    pub(crate) kinetics_path: &'a str,
    pub(crate) credentials_path: &'a str,
    pub(crate) provision_warn_threshold: &'a usize,
}

static BUILD_CONFIG: OnceLock<BuildConfig> = OnceLock::new();

pub(crate) fn build_config() -> Result<&'static BuildConfig<'static>, Error> {
    let home_dir = std::env::var("HOME").map_err(|_| {
        log::error!("Failed to get $HOME");

        Error::new(
            "$HOME is missing",
            Some("Your shell might not be supported."),
        )
    })?;

    Ok(BUILD_CONFIG.get_or_init(|| {
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

        BuildConfig {
            api_base,
            kinetics_path: build_path,
            credentials_path,
            provision_warn_threshold: &5,
        }
    }))
}

pub fn api_url(path: &str) -> String {
    format!("{}{}", build_config().unwrap().api_base, path)
}
