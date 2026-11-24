mod entrypoints;
mod features;
mod secret_loader;

pub(super) fn runtime_json() -> serde_json::Value {
    serde_json::json!({
        "schema_version": super::runtime::RUNTIME_CONFIG_VERSION,
        "owner_id": "owner",
        "cloud_account_id": "123456789012",
        "sqldb_cluster_id": "test-cluster",
        "sqldb_user": "test-user",
        "secrets_table_name": "custom-secrets-table",
        "project_scope": "owner#project",
        "package_scope": "owner#project#payments",
        "queues": {},
        "storage_bucket": "bucket",
        "storage_prefix": "project/"
    })
}
