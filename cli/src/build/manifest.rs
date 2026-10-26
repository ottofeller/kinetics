use eyre::{eyre, ContextCompat};
use kinetics_parser::{ParsedFunction, Role};
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

/// Merge runtime defaults while preserving dependency sources and existing features.
fn merge_dependency_group(doc: &mut toml_edit::DocumentMut, group: &str) -> eyre::Result<()> {
    let groups = LAMBDA_DEPENDENCIES.as_ref().map_err(|err| eyre!("{err}"))?;

    let group = groups
        .get(group)
        .ok_or_else(|| eyre!("Missing lambda dependency group {group}"))?;

    let dependencies = doc["dependencies"]
        .or_insert(toml_edit::Table::new().into())
        .as_table_mut()
        .ok_or_else(|| eyre!("Cargo.toml dependencies must be a table"))?;

    for (name, dependency) in group {
        let Some(existing) = dependencies.get_mut(name) else {
            dependencies.insert(name, dependency.clone());
            continue;
        };

        // Keep the source package's dependency settings and add only the features
        // required by the generated entry points.
        let Some(features) = dependency.get("features").and_then(Item::as_array) else {
            continue;
        };

        if existing.as_str().is_some() {
            *existing = toml_edit::Table::from_iter([("version", existing.clone())]).into();
        }

        let existing_features = existing["features"]
            .or_insert(toml_edit::Array::new().into())
            .as_array_mut()
            .ok_or_else(|| eyre!("Dependency {name} features must be an array"))?;

        for feature in features.iter().filter_map(toml_edit::Value::as_str) {
            if !existing_features
                .iter()
                .any(|existing| existing.as_str() == Some(feature))
            {
                existing_features.push(feature);
            }
        }
    }

    Ok(())
}

/// Set the library name and runtime dependencies, returning the name used in imports.
pub(super) fn configure_package(
    doc: &mut toml_edit::DocumentMut,
    functions: &[ParsedFunction],
) -> eyre::Result<String> {
    let lib_name = doc
        .get("lib")
        .and_then(toml_edit::Item::as_table)
        .and_then(|lib| lib.get("name"))
        .and_then(toml_edit::Item::as_str)
        .map(String::from)
        .or_else(|| {
            doc.get("package")
                .and_then(toml_edit::Item::as_table)
                .and_then(|package| package.get("name"))
                .and_then(toml_edit::Item::as_str)
                .map(|name| name.replace('-', "_"))
        })
        .wrap_err("Package name is missing from Cargo.toml")?;

    doc.entry("lib")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .wrap_err("Invalid [lib] table in Cargo.toml")?
        .insert("name", toml_edit::value(&lib_name));
    add_runtime_dependencies(doc, functions)?;

    Ok(lib_name)
}

/// Add workload runtime dependencies and remove the separate `kinetics-macro` dependency.
fn add_runtime_dependencies(
    doc: &mut toml_edit::DocumentMut,
    functions: &[ParsedFunction],
) -> eyre::Result<()> {
    merge_dependency_group(doc, "common")?;

    let mut added = std::collections::BTreeSet::new();
    for function in functions {
        let group = match function.role {
            Role::Cron | Role::Worker | Role::DlqWorker => "cron",
            Role::Endpoint => "endpoint",
        };
        if added.insert(group) {
            merge_dependency_group(doc, group)?;
        }
    }

    let kinetics_version = env!("CARGO_PKG_VERSION");
    if doc["dependencies"]["kinetics-lib"].as_str().is_some() {
        // Discard string version and write an object
        doc["dependencies"]["kinetics-lib"] =
            toml_edit::Table::from_iter([("version", kinetics_version)]).into();
    } else {
        // For an object overwrite only the version field
        doc["dependencies"]["kinetics-lib"]
            .or_insert(toml_edit::Table::new().into())
            .as_table_mut()
            .map(|t| t.insert("version", kinetics_version.into()));
    }

    if let Some(deps_table) = doc["dependencies"].as_table_mut() {
        deps_table.remove("kinetics-macro");
    }

    Ok(())
}
