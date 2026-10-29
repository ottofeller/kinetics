use crate::tools::{
    features::secrets::SecretsV2,
    runtime::{RuntimeConfig, RUNTIME_CONFIG_VERSION},
};
use serde_json::json;

pub(super) fn runtime_json() -> serde_json::Value {
    json!({
        "schema_version": RUNTIME_CONFIG_VERSION,
        "owner_id": "owner",
        "project_name": "project",
        "cloud_account_id": "123456789012",
        "sqldb_cluster_id": "",
        "sqldb_user": "",
        "secrets_table_name": "custom-secrets-table",
        "secrets_names": ["legacy-parameter"],
        "queues": {},
        "storage_bucket": "bucket",
        "storage_prefix": "project/"
    })
}

#[test]
fn legacy_runtime_defaults_to_disabled_features() {
    let config: RuntimeConfig = serde_json::from_value(runtime_json()).unwrap();
    assert!(config.package_name.is_none());
    assert!(serde_json::to_value(&config)
        .unwrap()
        .get("package_name")
        .is_none());

    assert!(!config.features.enabled::<SecretsV2>());
}

#[tokio::test]
async fn disabled_dynamodb_performs_no_requests_even_with_legacy_secrets() {
    let sdk_config = aws_config::SdkConfig::builder().build();
    for package in [None, Some("payments")] {
        let mut json = runtime_json();
        json["package_name"] = json!(package);
        json["features"] = json!([]);
        let config: RuntimeConfig = serde_json::from_value(json).unwrap();

        let secrets = crate::tools::secrets::load(&sdk_config, &config)
            .await
            .unwrap();
        assert!(secrets.is_empty());
    }
}

#[test]
#[allow(deprecated)]
fn reads_package_identity_with_or_without_legacy_names() {
    let mut json = runtime_json();
    json["package_name"] = json!("payments");
    json["features"] = json!(["secrets_v2"]);
    let config: RuntimeConfig = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(config.package_name.as_deref(), Some("payments"));
    assert!(config.features.enabled::<SecretsV2>());
    assert_eq!(config.secrets_table_name, "custom-secrets-table");
    assert_eq!(config.secrets_names, ["legacy-parameter"]);

    json.as_object_mut().unwrap().remove("secrets_names");
    let config: RuntimeConfig = serde_json::from_value(json).unwrap();
    assert!(config.secrets_names.is_empty());
}
