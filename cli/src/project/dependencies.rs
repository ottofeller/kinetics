use eyre::eyre;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use toml_edit::Item;

type Dependency = (String, Item);
type DependencyGroups = HashMap<String, Vec<Dependency>>;

/// Parsed lambda dependency groups from the embedded CLI's Cargo.toml
static LAMBDA_DEPENDENCIES: Lazy<Result<DependencyGroups, String>> = Lazy::new(|| {
    // Embedded Cargo.toml is parsed at compile-time and stored as a static string
    let manifest: toml_edit::DocumentMut =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
            .parse()
            .map_err(|err| format!("Failed to parse embedded Cargo.toml: {err}"))?;

    let groups = manifest["package"]["metadata"]["kinetics"]["dependencies"]
        .as_table()
        .ok_or("Missing package.metadata.kinetics.dependencies in embedded Cargo.toml")?;

    groups
        .iter()
        .map(|(group_name, group)| {
            let group = group.as_table().ok_or_else(|| {
                format!("Expected lambda dependency group {group_name} to be a table")
            })?;

            let dependencies = group
                .iter()
                .map(|(name, dependency)| (name.to_string(), dependency.clone()))
                .collect();

            Ok((group_name.to_string(), dependencies))
        })
        .collect()
});

/// Inserts a lambda dependency group into the lambda's Cargo.toml dependencies section
pub fn insert_lambda_dependency_group(
    doc: &mut toml_edit::DocumentMut,
    group: &str,
) -> eyre::Result<()> {
    let groups = LAMBDA_DEPENDENCIES.as_ref().map_err(|err| eyre!("{err}"))?;

    let group = groups
        .get(group)
        .ok_or_else(|| eyre!("Missing lambda dependency group {group}"))?;

    let dependencies = doc["dependencies"]
        .or_insert(toml_edit::Table::new().into())
        .as_table_mut()
        .ok_or_else(|| eyre!("Cargo.toml dependencies must be a table"))?;

    for (name, dependency) in group {
        dependencies.insert(name, dependency.clone());
    }

    Ok(())
}
