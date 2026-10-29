use super::runtime::runtime_json;
use crate::tools::features::{
    secrets::{SecretsParams, SecretsV2},
    Feature, Features,
};
use crate::tools::runtime::RuntimeConfig;
use crate::tools::secrets;
use serde_json::json;

#[test]
fn future_features_do_not_break_older_runtimes() {
    let mut json = runtime_json();
    json["features"] = json!(["future_feature", "secrets_v2"]);
    let config: RuntimeConfig = serde_json::from_value(json).unwrap();
    assert!(config.features.enabled::<SecretsV2>());
}

#[tokio::test]
async fn secrets_v2_requires_the_flag_and_package_identity() {
    let mut valid = runtime_json();
    valid["package_name"] = json!("payments");
    valid["features"] = json!(Features::default().with::<SecretsV2>(SecretsParams {
        package: Some("payments"),
    }));
    let config: RuntimeConfig = serde_json::from_value(valid.clone()).unwrap();
    assert!(config.features.enabled::<SecretsV2>());
    assert!(!SecretsV2::enabled(SecretsParams { package: None }));

    let sdk_config = aws_config::SdkConfig::builder().build();
    for (package, features) in [
        (Some("payments"), Features::default()),
        (
            None,
            Features::default().with::<SecretsV2>(SecretsParams { package: None }),
        ),
    ] {
        let mut invalid = valid.clone();
        invalid["package_name"] = json!(package);
        invalid["features"] = json!(features);
        let config: RuntimeConfig = serde_json::from_value(invalid).unwrap();

        assert!(!config.features.enabled::<SecretsV2>());
        assert!(secrets::load(&sdk_config, &config)
            .await
            .unwrap()
            .is_empty());
    }
}
