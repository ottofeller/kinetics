use std::sync::OnceLock;

pub(crate) struct Config<'a> {
    pub(crate) api_base: &'a str,
    pub(crate) username: &'a str,
    pub(crate) username_escaped: &'a str,
    pub(crate) cloud_front_domain: Option<&'a str>,
    pub(crate) s3_bucket_name: &'a str,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub(crate) fn config() -> &'static Config<'static> {
    let use_production_domain =
        option_env!("USE_PRODUCTION_CLOUDFRONT_DOMAIN").unwrap_or("true") == "true";

    CONFIG.get_or_init(|| Config {
        cloud_front_domain: if use_production_domain {
            Some("usekinetics.com")
        } else {
            None
        },
        api_base: option_env!("KINETICS_API_BASE").unwrap_or("https://backend.usekinetics.com/"),
        username: option_env!("KINETICS_USERNAME").unwrap_or("artem@ottofeller.com"),
        username_escaped: option_env!("KINETICS_USERNAME_ESCAPED")
            .unwrap_or("artemATottofellerDOTcom"),
        s3_bucket_name: option_env!("KINETICS_S3_BUCKET_NAME").unwrap_or("kinetics-rust-builds"),
    })
}
