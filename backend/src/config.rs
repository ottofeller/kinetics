use std::sync::OnceLock;

pub(crate) struct Config<'a> {
    pub(crate) cloud_front_domain: Option<&'a str>,
    pub(crate) s3_bucket_name: &'a str,
    pub(crate) kms_key_id: &'a str,
    pub(crate) hosted_zone_id: &'a str,
    pub(crate) lambda_credentials_role_arn: &'a str,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub(crate) fn config() -> &'static Config<'static> {
    let use_production_domain =
        option_env!("KINETICS_USE_PRODUCTION_DOMAIN").unwrap_or("true") == "true";

    CONFIG.get_or_init(|| Config {
        kms_key_id: option_env!("KINETICS_KMS_KEY_ID")
            .unwrap_or("f1e08622-51cb-4868-adf5-9bb8aa8c0a87"),

        cloud_front_domain: if use_production_domain {
            Some("usekinetics.com")
        } else {
            None
        },
        s3_bucket_name: option_env!("KINETICS_S3_BUCKET_NAME")
            .unwrap_or("kinetics-rust-builds-production"),
        hosted_zone_id: option_env!("KINETICS_HOSTED_ZONE_ID").unwrap_or("Z05200421KHLZSXGM7STA"),
        lambda_credentials_role_arn: option_env!("KINETICS_LAMBDA_CREDENTIALS_ROLE_ARN")
            .unwrap_or("arn:aws:iam::430118855033:role/artemATottofellerDOTcom-b-EndpointRoleartemATottofe-Unx3jshlYJGX"),
    })
}
