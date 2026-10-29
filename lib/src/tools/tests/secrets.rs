use crate::tools::secrets::{merge_secrets, scope_key, Secret, Source};
use aws_sdk_dynamodb::types::AttributeValue;
use std::collections::HashMap;

fn secret(scope: &str, key: &str, value: &str) -> HashMap<String, AttributeValue> {
    serde_dynamo::to_item(Secret {
        scope: scope.to_string(),
        key: key.to_string(),
        value: value.to_string(),
    })
    .unwrap()
}

#[test]
fn scope_keys_keep_identity_components_distinct() {
    assert_ne!(
        scope_key("owner#team", "shop", None),
        scope_key("owner", "team#shop", None)
    );
    assert_ne!(
        scope_key("owner", "shop", Some("payments")),
        scope_key("owner", "shop#payments", None)
    );
    assert_ne!(
        scope_key("owner%23team", "shop", None),
        scope_key("owner#team", "shop", None)
    );
}

#[test]
fn scope_keys_escape_iam_wildcards() {
    let key = scope_key("owner*", "shop?", Some("payments"));
    assert!(!key.contains('*'));
    assert!(!key.contains('?'));
}

#[test]
fn package_overrides_project_regardless_of_response_order() {
    let values = merge_secrets(
        &["project", "package"],
        vec![
            secret("package", "local#TOKEN", "package-token"),
            secret("project", "local#TOKEN", "project-token"),
            secret("project", "local#HOST", "host"),
        ],
    )
    .unwrap();

    assert_eq!(values["TOKEN"], "package-token");
    assert_eq!(values["HOST"], "host");
}

#[test]
fn empty_scopes_are_allowed() {
    assert!(merge_secrets(&["project", "package"], vec![])
        .unwrap()
        .is_empty());
    let values = merge_secrets(
        &["project", "package"],
        vec![secret("project", "local#TOKEN", "project-token")],
    )
    .unwrap();
    assert_eq!(values["TOKEN"], "project-token");
}

#[test]
fn invalid_values_are_rejected_without_exposing_their_content() {
    let mut item = secret("project", "local#TOKEN", "value");
    item.insert(
        "value".to_string(),
        AttributeValue::L(vec![AttributeValue::S("sensitive-content".to_string())]),
    );
    let error = merge_secrets(&["project"], vec![item]).unwrap_err();

    assert!(!format!("{error:?}").contains("sensitive-content"));
}

#[test]
fn all_record_fields_are_required() {
    for field in ["scope", "key", "value"] {
        let mut item = secret("project", "local#TOKEN", "value");
        item.remove(field);
        assert!(merge_secrets(&["project"], vec![item]).is_err());
    }
}

#[test]
fn records_outside_requested_scopes_are_ignored() {
    assert!(merge_secrets(
        &["project", "package"],
        vec![secret("another-package", "local#TOKEN", "value")],
    )
    .unwrap()
    .is_empty());
}

#[test]
fn keys_require_a_known_source_prefix_and_a_name() {
    for key in ["local#", "TOKEN", "unknown#TOKEN"] {
        assert!(merge_secrets(&["project"], vec![secret("project", key, "value")],).is_err());
    }
}

#[test]
fn shared_source_uses_the_same_record_schema() {
    let values = merge_secrets(
        &["project", "package"],
        vec![
            secret("project", &Source::Shared.key("TOKEN"), "project-token"),
            secret("package", &Source::Shared.key("TOKEN"), "package-token"),
        ],
    )
    .unwrap();

    assert_eq!(values["TOKEN"], "package-token");
}
