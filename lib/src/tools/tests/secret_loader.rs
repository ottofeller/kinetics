use super::runtime::runtime_json;
use crate::tools::{runtime::RuntimeConfig, secrets};
use aws_sdk_dynamodb::{operation::query::QueryOutput, types::AttributeValue};
use aws_sdk_ssm::{
    operation::{
        get_parameter::GetParameterOutput, list_tags_for_resource::ListTagsForResourceOutput,
    },
    types::{Parameter, ResourceTypeForTagging, Tag},
};
use aws_smithy_mocks::{mock, mock_client, Rule, RuleMode};
use serde_json::json;

#[tokio::test]
async fn legacy_config_loads_ssm_values_using_original_names() {
    let get_parameter = mock!(aws_sdk_ssm::Client::get_parameter)
        .match_requests(|request| {
            request.name() == Some("legacy-parameter") && request.with_decryption() == Some(true)
        })
        .then_output(|| {
            GetParameterOutput::builder()
                .parameter(Parameter::builder().value("ssm-value").build())
                .build()
        });
    let list_tags = mock!(aws_sdk_ssm::Client::list_tags_for_resource)
        .match_requests(|request| {
            request.resource_id() == Some("legacy-parameter")
                && request.resource_type() == Some(&ResourceTypeForTagging::Parameter)
        })
        .then_output(|| {
            ListTagsForResourceOutput::builder()
                .tag_list(
                    Tag::builder()
                        .key("original_name")
                        .value("TOKEN")
                        .build()
                        .unwrap(),
                )
                .build()
        });
    let ssm = mock_client!(aws_sdk_ssm, [&get_parameter, &list_tags]);
    let mut json = runtime_json();
    json.as_object_mut().unwrap().remove("secrets_table_name");
    let config: RuntimeConfig = serde_json::from_value(json).unwrap();

    let values = secrets::load_ssm(&ssm, &config).await.unwrap();

    assert_eq!(get_parameter.num_calls(), 1);
    assert_eq!(list_tags.num_calls(), 1);
    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!(values);
    });
}

#[tokio::test]
async fn dynamodb_loads_both_scopes_with_package_precedence() {
    let project = query_rule(
        "owner#project",
        &[("TOKEN", "project-value"), ("HOST", "project-host")],
    );
    let package = query_rule("owner#project#payments", &[("TOKEN", "package-value")]);
    let dynamodb = mock_client!(aws_sdk_dynamodb, RuleMode::MatchAny, [&project, &package]);
    let mut json = runtime_json();
    json["package_name"] = json!("payments");
    json["features"] = json!(["secrets_v2"]);
    let config: RuntimeConfig = serde_json::from_value(json).unwrap();

    let values = secrets::load_dynamodb(&dynamodb, &config).await.unwrap();

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
                && request.consistent_read() == Some(true)
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
