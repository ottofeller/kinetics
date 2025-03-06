use std::sync::OnceLock;

pub(crate) struct Config<'a> {
    pub(crate) cloud_front_domain: Option<&'a str>,
    pub(crate) s3_bucket_name: &'a str,
    pub(crate) kms_key_id: &'a str,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub(crate) fn config() -> &'static Config<'static> {
    let use_production_domain =
        option_env!("KINETICS_USE_PRODUCTION_DOMAIN").unwrap_or("true") == "true";

    CONFIG.get_or_init(|| Config {
        kms_key_id: option_env!("KINETICS_KMS_KEY_ID")
            .unwrap_or("1bf38d51-e7e3-4c20-b155-60c6214b0255"),

        cloud_front_domain: if use_production_domain {
            Some("usekinetics.com")
        } else {
            None
        },
        s3_bucket_name: option_env!("KINETICS_S3_BUCKET_NAME").unwrap_or("kinetics-rust-builds"),
    })
}
