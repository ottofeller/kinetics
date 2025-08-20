use crate::error::Error;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug)]
pub(crate) struct CloudConfig<'a> {
    // Cloud provider account ID
    #[allow(dead_code)]
    pub(crate) account_id: &'a str,
}

#[derive(Debug)]
pub(crate) struct BuildConfig<'a> {
    pub(crate) api_base: &'a str,
    pub(crate) domain: &'a str,
    pub(crate) build_path: &'a str,
    pub(crate) credentials_path: &'a str,
}

#[allow(dead_code)]
static CLOUD_CONFIG: OnceLock<CloudConfig> = OnceLock::new();

static BUILD_CONFIG: OnceLock<BuildConfig> = OnceLock::new();

#[allow(dead_code)]
pub(crate) fn cloud_config() -> Result<&'static CloudConfig<'static>, Error> {
    Ok(CLOUD_CONFIG.get_or_init(|| CloudConfig {
        account_id: "430118855033",
    }))
}

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
            build_path,
            credentials_path,
            domain: "usekinetics.com",
        }
    }))
}

pub fn api_url(path: &str) -> String {
    format!("{}{}", build_config().unwrap().api_base, path)
}
