use super::runtime::RuntimeConfig;
use aws_config::SdkConfig;
use aws_sdk_dynamodb::{types::AttributeValue, Client};
use eyre::WrapErr;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const TABLE_NAME: &str = "kinetics-secrets";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Local,
    Shared,
}

impl Source {
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Local => "local#",
            Self::Shared => "shared#",
        }
    }

    pub fn key(self, name: &str) -> String {
        format!("{}{name}", self.prefix())
    }
}

type Item = HashMap<String, AttributeValue>;

#[derive(Deserialize, Serialize)]
struct Secret {
    scope: String,
    key: String,
    value: String,
}

/// Shared partition-key format for secret readers, writers, and IAM policies.
pub fn scope_key(owner_id: &str, project_name: &str, package_name: Option<&str>) -> String {
    // Encode components independently so separators and IAM wildcards remain literal data.
    let encode = |value: &str| utf8_percent_encode(value, NON_ALPHANUMERIC).to_string();
    let mut key = format!("{}#{}", encode(owner_id), encode(project_name));
    if let Some(package_name) = package_name {
        key.push('#');
        key.push_str(&encode(package_name));
    }
    key
}

/// Read local secrets for initialization of a Lambda execution environment.
///
/// Each item contains `scope`, `key = "local#NAME"`, and a string `value`. Project
/// and package scopes are queried concurrently, including all pages. Package values
/// override project values. The caller retains the returned snapshot.
pub async fn load(
    sdk_config: &SdkConfig,
    config: &RuntimeConfig,
) -> eyre::Result<HashMap<String, String>> {
    let source = Source::Local;
    let project_scope = scope_key(&config.owner_id, &config.project_name, None);
    let package_scope = scope_key(
        &config.owner_id,
        &config.project_name,
        Some(&config.package_name),
    );
    let client = Client::new(sdk_config);
    let (mut items, package_items) = tokio::try_join!(
        query_scope(&client, &project_scope, source),
        query_scope(&client, &package_scope, source)
    )?;
    items.extend(package_items);
    merge_secrets(&[&project_scope, &package_scope], source, items)
}

async fn query_scope(client: &Client, scope: &str, source: Source) -> eyre::Result<Vec<Item>> {
    let mut items = Vec::new();
    let mut start_key = None;

    loop {
        let response = client
            .query()
            .table_name(TABLE_NAME)
            .key_condition_expression("#scope = :scope AND begins_with(#key, :prefix)")
            .expression_attribute_names("#scope", "scope")
            .expression_attribute_names("#key", "key")
            .expression_attribute_values(":scope", AttributeValue::S(scope.to_string()))
            .expression_attribute_values(":prefix", AttributeValue::S(source.prefix().to_string()))
            .consistent_read(true)
            .set_exclusive_start_key(start_key)
            .send()
            .await
            .wrap_err("Failed to read secrets from DynamoDB")?;

        items.extend(response.items.unwrap_or_default());
        start_key = response.last_evaluated_key.filter(|key| !key.is_empty());
        if start_key.is_none() {
            return Ok(items);
        }
    }
}

pub(super) fn merge_secrets(
    scopes: &[&str],
    source: Source,
    items: Vec<Item>,
) -> eyre::Result<HashMap<String, String>> {
    let records: Vec<Secret> = serde_dynamo::from_items(items)
        .map_err(|_| eyre::eyre!("Failed to deserialize secret records"))?;

    let mut scoped_values: HashMap<String, HashMap<String, String>> = HashMap::new();
    for Secret { scope, key, value } in records {
        eyre::ensure!(
            scopes.contains(&scope.as_str()),
            "Secret has an unexpected scope"
        );

        let name = key
            .strip_prefix(source.prefix())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| eyre::eyre!("Secret has an invalid key for the requested source"))?;

        scoped_values
            .entry(scope)
            .or_default()
            .insert(name.to_string(), value);
    }

    // Scope order defines precedence, independently of item order.
    let mut secrets = HashMap::new();
    for scope in scopes {
        if let Some(values) = scoped_values.remove(*scope) {
            secrets.extend(values);
        }
    }
    Ok(secrets)
}
