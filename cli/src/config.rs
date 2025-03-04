use std::sync::OnceLock;

const API_BASE: &str = "https://backend.usekinetics.com/";
const USERNAME: &str = "artem@ottofeller.com";
const USERNAME_ESCAPED: &str = "artemATottofellerDOTcom";
const CLOUD_FRONT_DOMAIN: &str = "usekinetics.com";
const S3_BUCKET_NAME: &str = "kinetics-rust-builds";

pub(crate) struct Config<'a> {
    pub(crate) api_base: &'a str,
    pub(crate) username: &'a str,
    pub(crate) username_escaped: &'a str,
    pub(crate) cloud_front_domain: Option<&'a str>,
    pub(crate) s3_bucket_name: &'a str,
}

pub(crate) fn config() -> &'static Config<'static> {
    static CONFIG: OnceLock<Config> = OnceLock::new();

    let use_production_domain =
        option_env!("USE_PRODUCTION_CLOUDFRONT_DOMAIN").unwrap_or("false") == "true";

    CONFIG.get_or_init(|| Config {
        cloud_front_domain: if use_production_domain {
            Some(CLOUD_FRONT_DOMAIN)
        } else {
            None
        },
        api_base: option_env!("KINETICS_API_BASE").unwrap_or(API_BASE),
        username: option_env!("KINETICS_USERNAME").unwrap_or(USERNAME),
        username_escaped: option_env!("KINETICS_USERNAME_ESCAPED").unwrap_or(USERNAME_ESCAPED),
        s3_bucket_name: option_env!("KINETICS_S3_BUCKET_NAME").unwrap_or(S3_BUCKET_NAME),
    })
}
