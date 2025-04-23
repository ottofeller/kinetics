use crate::error::Error;
use std::path::Path;
use std::sync::OnceLock;

#[cfg(feature = "enable-direct-deploy")]
#[derive(Debug)]
pub(crate) struct Config<'a> {
    pub(crate) api_base: &'a str,
    pub(crate) build_path: &'a str,
    pub(crate) credentials_path: &'a str,
    pub(crate) username: &'a str,
    pub(crate) username_escaped: &'a str,
    pub(crate) cloud_front_domain: Option<&'a str>,
    pub(crate) kms_key_id: &'a str,
    pub(crate) s3_bucket_name: &'a str,
    pub(crate) hosted_zone_id: Option<&'a str>,
    pub(crate) lambda_credentials_role_arn: &'a str,
}

#[cfg(not(feature = "enable-direct-deploy"))]
#[derive(Debug)]
pub(crate) struct Config<'a> {
    pub(crate) api_base: &'a str,
    pub(crate) build_path: &'a str,
    pub(crate) credentials_path: &'a str,
}

pub const DIRECT_DEPLOY_ENABLED: bool = cfg!(feature = "enable-direct-deploy");

static CONFIG: OnceLock<Config> = OnceLock::new();

pub(crate) fn build_config() -> Result<&'static Config<'static>, Error> {
    CONFIG.get_or_try_init(|| {
        let api_base =
            option_env!("KINETICS_API_BASE").unwrap_or("https://backend.usekinetics.com/");

        let home_dir = match option_env!("HOME") {
            Some(home_dir) => home_dir,
            None => {
                log::error!("Failed to get $HOME");

                return Err(Error::new(
                    "$HOME is missing",
                    Some("Your shell might not be supported."),
                ))
            }
        };

        let build_path_raw = Path::new(home_dir).join(".kinetics");

        // Create a static string to avoid referencing temporary value
        let build_path = Box::leak(
            build_path_raw.display().to_string().into_boxed_str(),
        );

        let credentials_path = Box::leak(
            build_path_raw.join(".credentials").display().to_string().into_boxed_str(),
        );

        #[cfg(feature = "enable-direct-deploy")]
        {
            let use_production_domain =
                option_env!("KINETICS_USE_PRODUCTION_DOMAIN").unwrap_or("true") == "true";

            Ok(Config {
                api_base,
                build_path,
                credentials_path,
                cloud_front_domain: if use_production_domain {
                    Some("usekinetics.com")
                } else {
                    None
                },
                username: option_env!("KINETICS_USERNAME").unwrap_or("artem@ottofeller.com"),
                username_escaped: option_env!("KINETICS_USERNAME_ESCAPED")
                    .unwrap_or("artemATottofellerDOTcom"),
                s3_bucket_name: option_env!("KINETICS_S3_BUCKET_NAME")
                    .unwrap_or("kinetics-rust-builds-production"),
                kms_key_id: option_env!("KINETICS_KMS_KEY_ID")
                    .unwrap_or("f1e08622-51cb-4868-adf5-9bb8aa8c0a87"),
                hosted_zone_id: option_env!("KINETICS_HOSTED_ZONE_ID"),
                lambda_credentials_role_arn: option_env!("KINETICS_LAMBDA_CREDENTIALS_ROLE_ARN")
                    .unwrap_or("arn:aws:iam::430118855033:role/artemATottofellerDOTcom-b-EndpointRoleartemATottofe-Unx3jshlYJGX"),
            })
        }

        #[cfg(not(feature = "enable-direct-deploy"))]
        {
            Ok(Config { api_base, build_path, credentials_path })
        }
    })
}
