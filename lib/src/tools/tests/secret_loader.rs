use crate::tools::secrets;
use aws_sdk_dynamodb::{operation::query::QueryOutput, types::AttributeValue};
use aws_smithy_mocks::{Rule, RuleMode, mock, mock_client};

#[tokio::test]
async fn dynamodb_loads_both_scopes_with_package_precedence() {
    let project = query_rule(
        "owner#project",
        &[("TOKEN", "project-value"), ("HOST", "project-host")],
    );
    let package = query_rule("owner#project#payments", &[("TOKEN", "package-value")]);
    let dynamodb = mock_client!(aws_sdk_dynamodb, RuleMode::MatchAny, [&project, &package]);
    let config = serde_json::from_value(super::runtime_json()).unwrap();

    let values = secrets::load(&dynamodb, &config).await.unwrap();

    assert_eq!(project.num_calls(), 1);
    assert_eq!(package.num_calls(), 1);
    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!(values);
    });
}

fn query_rule(scope: &'static str, values: &'static [(&str, &str)]) -> Rule {
    mock!(aws_sdk_dynamodb::Client::query)
        .match_requests(move |request| {
            request.table_name() == Some("custom-secrets-table")
                && request.key_condition_expression() == Some("#scope = :scope")
                && request
                    .expression_attribute_names()
                    .and_then(|names| names.get("#scope"))
                    .map(String::as_str)
                    == Some("scope")
                && request
                    .expression_attribute_values()
                    .and_then(|values| values.get(":scope"))
                    == Some(&AttributeValue::S(scope.to_string()))
        })
        .then_output(move || {
            QueryOutput::builder()
                .set_items(Some(
                    values
                        .iter()
                        .map(|(name, value)| {
                            serde_dynamo::to_item(secrets::Secret {
                                scope: scope.to_string(),
                                key: secrets::Source::Local.key(name),
                                value: value.to_string(),
                            })
                            .unwrap()
                        })
                        .collect(),
                ))
                .build()
        })
}

#[tokio::test]
async fn reads_all_pages_before_merging_scopes() {
    use aws_sdk_dynamodb::types::AttributeValue::S;
    use std::collections::HashMap;

    let continuation = HashMap::from([("scope".into(), S("owner#project".into()))]);
    let first = mock!(aws_sdk_dynamodb::Client::query)
        .match_requests(|request| {
            request.exclusive_start_key().is_none()
                && request
                    .expression_attribute_values()
                    .and_then(|values| values.get(":scope"))
                    == Some(&S("owner#project".into()))
        })
        .then_output(move || {
            QueryOutput::builder()
                .items(
                    serde_dynamo::to_item(secrets::Secret {
                        scope: "owner#project".into(),
                        key: secrets::Source::Local.key("HOST"),
                        value: "first-page".into(),
                    })
                    .unwrap(),
                )
                .set_last_evaluated_key(Some(continuation.clone()))
                .build()
        });
    let next = mock!(aws_sdk_dynamodb::Client::query)
        .match_requests(|request| request.exclusive_start_key().is_some())
        .then_output(|| {
            QueryOutput::builder()
                .items(
                    serde_dynamo::to_item(secrets::Secret {
                        scope: "owner#project".into(),
                        key: secrets::Source::Local.key("TOKEN"),
                        value: "next-page".into(),
                    })
                    .unwrap(),
                )
                .set_last_evaluated_key(Some(HashMap::new()))
                .build()
        });
    let package = query_rule("owner#project#payments", &[("HOST", "package-host")]);
    let client = mock_client!(
        aws_sdk_dynamodb,
        RuleMode::MatchAny,
        [&first, &next, &package]
    );
    let config = super::runtime_json();
    let values = secrets::load(&client, &serde_json::from_value(config).unwrap())
        .await
        .unwrap();
    assert_eq!(first.num_calls(), 1);
    assert_eq!(next.num_calls(), 1);
    assert_eq!(package.num_calls(), 1);
    assert_eq!(values["TOKEN"], "next-page");
    assert_eq!(values["HOST"], "package-host");
}
