use crate::tools::runtime::{RuntimeConfig, RUNTIME_CONFIG_VERSION};
use serde_json::json;

fn runtime_json() -> serde_json::Value {
    json!({
        "schema_version": RUNTIME_CONFIG_VERSION,
        "owner_id": "owner",
        "project_name": "project",
        "cloud_account_id": "123456789012",
        "sqldb_cluster_id": "",
        "sqldb_user": "",
        "secrets_names": ["legacy-parameter"],
        "queues": {},
        "storage_bucket": "bucket",
        "storage_prefix": "project/"
    })
}

#[test]
fn new_runtime_requires_package_identity() {
    let error = serde_json::from_value::<RuntimeConfig>(runtime_json()).unwrap_err();
    assert!(error.to_string().contains("package_name"));
}

#[test]
#[allow(deprecated)]
fn reads_package_identity_with_or_without_legacy_names() {
    let mut json = runtime_json();
    json["package_name"] = json!("payments");
    let config: RuntimeConfig = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(config.package_name, "payments");
    assert_eq!(config.secrets_names, ["legacy-parameter"]);

    json.as_object_mut().unwrap().remove("secrets_names");
    let config: RuntimeConfig = serde_json::from_value(json).unwrap();
    assert!(config.secrets_names.is_empty());
}
