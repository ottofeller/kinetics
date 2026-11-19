use super::RuntimeConfig;
use crate::tools::tests::runtime_json;
use aws_sdk_dynamodb::{operation::get_item::GetItemOutput, types::AttributeValue::S};
use aws_smithy_mocks::{mock, mock_client};
use serde_json::json;

#[tokio::test]
async fn dynamodb_snapshot_read_validates_presence_and_schema_version() {
    let mut unsupported = runtime_json();
    unsupported["schema_version"] = json!(999);
    for (body, expected_error) in [
        (None, Some("Runtime configuration not found")),
        (Some(json!({})), Some("Invalid runtime configuration")),
        (
            Some(unsupported),
            Some("Unsupported runtime configuration schema version: 999"),
        ),
        (Some(runtime_json()), None),
    ] {
        let rule = mock!(aws_sdk_dynamodb::Client::get_item)
            .match_requests(|request| {
                request.table_name() == Some("kinetics")
                    && request.key().and_then(|key| key.get("id"))
                        == Some(&S("runtime-config#owner#hash".into()))
            })
            .then_output(move || match &body {
                None => GetItemOutput::builder().build(),
                Some(body) => GetItemOutput::builder()
                    .set_item(Some(serde_dynamo::to_item(body).unwrap()))
                    .build(),
            });
        let client = mock_client!(aws_sdk_dynamodb, [&rule]);
        let result = RuntimeConfig::fetch(&client, "kinetics", "runtime-config#owner#hash").await;
        match expected_error {
            Some(message) => {
                let error = result.unwrap_err();
                assert_eq!(error.to_string(), message);
                if message == "Invalid runtime configuration" {
                    assert!(error.chain().nth(1).is_some());
                }
            }
            None => assert_eq!(result.unwrap().project_scope, "owner#project"),
        }
        assert_eq!(rule.num_calls(), 1);
    }
}
