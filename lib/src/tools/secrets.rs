use super::{features::secrets::SecretsV2, runtime::RuntimeConfig};
use aws_config::SdkConfig;
use aws_sdk_dynamodb::{types::AttributeValue, Client};
use eyre::WrapErr;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
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
pub struct Secret {
    pub scope: String,
    pub key: String,
    pub value: String,
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

/// Read shared and local secrets when the backend enables DynamoDB secrets.
///
/// Disabled deployments perform no requests. Both scopes are queried concurrently,
/// each returning all sources and pages. Local values override shared values;
/// within each source, package values override project values.
pub async fn load(
    sdk_config: &SdkConfig,
    config: &RuntimeConfig,
) -> eyre::Result<HashMap<String, String>> {
    if !config.features.enabled::<SecretsV2>() {
        return Ok(HashMap::new());
    }

    let project_scope = scope_key(&config.owner_id, &config.project_name, None);

    let package_scope = scope_key(
        &config.owner_id,
        &config.project_name,
        config.package_name.as_deref(),
    );

    let client = Client::new(sdk_config);
    let (mut items, package_items) = tokio::try_join!(
        query_scope(&client, &config.secrets_table_name, &project_scope),
        query_scope(&client, &config.secrets_table_name, &package_scope),
    )?;
    items.extend(package_items);
    merge_secrets(&[&project_scope, &package_scope], items)
}

async fn query_scope(client: &Client, table_name: &str, scope: &str) -> eyre::Result<Vec<Item>> {
    let mut items = Vec::new();
    let mut start_key = None;

    loop {
        let response = client
            .query()
            .table_name(table_name)
            .key_condition_expression("#scope = :scope")
            .expression_attribute_names("#scope", "scope")
            .expression_attribute_values(":scope", AttributeValue::S(scope.to_string()))
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
    items: Vec<Item>,
) -> eyre::Result<HashMap<String, String>> {
    let records: Vec<Secret> = serde_dynamo::from_items(items)
        .map_err(|_| eyre::eyre!("Failed to deserialize secret records"))?;

    let mut scoped_values: HashMap<(Source, String), HashMap<String, String>> = HashMap::new();

    for Secret { scope, key, value } in records {
        let (source, name) = [Source::Shared, Source::Local]
            .into_iter()
            .find_map(|source| key.strip_prefix(source.prefix()).map(|name| (source, name)))
            .filter(|(_, name)| !name.is_empty())
            .ok_or_else(|| eyre::eyre!("Secret has an invalid source prefix or empty name"))?;

        scoped_values
            .entry((source, scope))
            .or_default()
            .insert(name.to_string(), value);
    }

    // Shared project -> shared package -> local project -> local package.
    let mut secrets = HashMap::new();
    for source in [Source::Shared, Source::Local] {
        for scope in scopes {
            if let Some(values) = scoped_values.remove(&(source, (*scope).to_string())) {
                secrets.extend(values);
            }
        }
    }

    Ok(secrets)
}
