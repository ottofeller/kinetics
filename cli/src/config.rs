use std::sync::OnceLock;

#[cfg(feature = "enable-direct-deploy")]
#[derive(Debug)]
pub(crate) struct Config<'a> {
    pub(crate) api_base: &'a str,
    pub(crate) username: &'a str,
    pub(crate) username_escaped: &'a str,
    pub(crate) cloud_front_domain: Option<&'a str>,
    pub(crate) s3_bucket_name: &'a str,
}

#[cfg(not(feature = "enable-direct-deploy"))]
#[derive(Debug)]
pub(crate) struct Config<'a> {
    pub(crate) api_base: &'a str,
}

pub const DIRECT_DEPLOY_ENABLED: bool = cfg!(feature = "enable-direct-deploy");

static CONFIG: OnceLock<Config> = OnceLock::new();

pub(crate) fn build_config() -> &'static Config<'static> {
    CONFIG.get_or_init(|| {
        let api_base =
            option_env!("KINETICS_API_BASE").unwrap_or("https://backend.usekinetics.com/");

        #[cfg(feature = "enable-direct-deploy")]
        {
            let use_production_domain =
                option_env!("KINETICS_USE_PRODUCTION_DOMAIN").unwrap_or("true") == "true";

            Config {
                cloud_front_domain: if use_production_domain {
                    Some("usekinetics.com")
                } else {
                    None
                },
                api_base,
                username: option_env!("KINETICS_USERNAME").unwrap_or("artem@ottofeller.com"),
                username_escaped: option_env!("KINETICS_USERNAME_ESCAPED")
                    .unwrap_or("artemATottofellerDOTcom"),
                s3_bucket_name: option_env!("KINETICS_S3_BUCKET_NAME")
                    .unwrap_or("kinetics-rust-builds-production"),
            }
        }

        #[cfg(not(feature = "enable-direct-deploy"))]
        {
            Config { api_base }
        }
    })
}

#[cfg(feature = "enable-direct-deploy")]
pub mod backend {
    // Re-export optional backend modules for direct deploy feature
    pub use ::backend::*;
}
