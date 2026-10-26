mod bundle;
mod cargo;
mod entrypoints;
mod exports;
mod files;
mod isolation;
mod manifest;
mod queue_calls;
mod rust_paths;
mod sources;

#[cfg(test)]
mod tests;

use crate::function::Function;
use crate::project::Project;
use eyre::WrapErr;
use files::BuildFiles;
use kinetics_parser::ParsedPackage;
use sources::PackageSources;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Generate the build workspace and return all functions, marking those selected.
pub fn generate_workspace(
    project: &Project,
    deploy_functions: &[String],
    package_name: Option<&str>,
) -> eyre::Result<Vec<Function>> {
    generate_workspace_into(
        project,
        deploy_functions,
        package_name,
        &project.build_path()?,
    )
}

/// Compile selected Lambda binaries and create their deployment archives.
pub async fn compile_and_bundle(
    functions: &[Function],
    progress: &indicatif::ProgressBar,
) -> eyre::Result<()> {
    cargo::compile(functions, progress).await?;
    let functions = functions.to_vec();
    let started = Instant::now();
    tokio::task::spawn_blocking(move || bundle::write(&functions))
        .await
        .wrap_err("Failed to join the Lambda bundling task")??;
    log::debug!(
        "Lambda ZIP packaging finished in {:.2}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// Original manifest and parsed sources for one workspace package.
struct SourcePackage {
    manifest: toml_edit::DocumentMut,
    parsed: ParsedPackage,
    lib_path: PathBuf,
}

/// Generate build inputs in the destination, preserving unchanged files.
fn generate_workspace_into(
    project: &Project,
    deploy_functions: &[String],
    package_name: Option<&str>,
    dst: &Path,
) -> eyre::Result<Vec<Function>> {
    let selected_package = package_name
        .map(|name| project.workspace.package(name))
        .transpose()?;

    if let (Some(package), Some(name)) = (selected_package, package_name) {
        if !project.workspace.is_standalone_crate
            && project.path != project.workspace.root_path
            && project.workspace.root_path.join(&package.relative_path) != project.path
        {
            eyre::bail!(
                "Package `{name}` is outside the current Kinetics project; use --project to select its project",
            );
        }
    }

    let src = &project.workspace.root_path;
    // Checksums of source files for preventing rewrite existing files
    let mut files = BuildFiles::new(dst.to_path_buf());

    let packages = read_packages(project)?;
    let mut all_functions = Vec::new();

    // Clone user project into the build folder.
    files.copy_tree(
        src,
        Path::new(""),
        // Ignore all workspace members - we copy them later
        &project
            .workspace
            .packages
            .iter()
            .map(|pkg| pkg.relative_path.clone())
            // Skip the root Cargo.toml, since we populate it ourselves.
            .chain([PathBuf::from("Cargo.toml")])
            .collect::<Vec<_>>(),
    )?;

    let mut workspace_doc: toml_edit::DocumentMut =
        fs::read_to_string(src.join("Cargo.toml"))?.parse()?;

    // Collect existing member paths for the workspace manifest
    let member_paths: Vec<String> = project
        .workspace
        .packages
        .iter()
        .map(|pkg| {
            let path = pkg.relative_path.to_string_lossy();
            if path.is_empty() {
                ".".to_string()
            } else {
                path.to_string()
            }
        })
        .collect();

    for package in &project.workspace.packages {
        let pkg_path = &package.relative_path;
        let source = &packages[pkg_path];
        let parsed_functions = &source.parsed.functions;
        let mut package_doc = source.manifest.clone();
        let sources = if parsed_functions.is_empty() {
            None
        } else {
            let lib_name = manifest::configure_package(&mut package_doc, parsed_functions)?;
            let references = project
                .workspace
                .references
                .iter()
                .filter(|reference| reference.package == *pkg_path && reference.source != *pkg_path)
                .map(|reference| (&packages[&reference.source].parsed, reference.name.as_str()))
                .collect::<Vec<_>>();
            Some(PackageSources::generate(
                &source.parsed,
                &source.lib_path,
                lib_name,
                &references,
            )?)
        };

        // Generated manifests and library exports must not be overwritten with
        // their source versions on every build: that would invalidate Cargo's cache.
        let mut skip_paths = Vec::new();
        if !parsed_functions.is_empty() || pkg_path.as_os_str().is_empty() {
            skip_paths.push(PathBuf::from("Cargo.toml"));
        }
        if let Some(sources) = &sources {
            skip_paths.extend(sources.shared.keys().cloned());
        }
        if pkg_path.as_os_str().is_empty() {
            skip_paths.extend(
                project
                    .workspace
                    .packages
                    .iter()
                    .filter(|pkg| !pkg.relative_path.as_os_str().is_empty())
                    .map(|pkg| pkg.relative_path.clone()),
            );
        }

        files.copy_tree(&src.join(pkg_path), pkg_path, &skip_paths)?;

        let Some(sources) = sources else { continue };
        for (path, syntax) in &sources.shared {
            files.write(&pkg_path.join(path), prettyplease::unparse(syntax))?;
        }
        for (path, syntax) in &sources.handlers {
            files.write(
                &pkg_path.join(".kinetics/handlers").join(path),
                prettyplease::unparse(syntax),
            )?;
        }
        if pkg_path.as_os_str().is_empty() {
            workspace_doc = package_doc;
        } else {
            files.write(&pkg_path.join("Cargo.toml"), package_doc.to_string())?;
        }
        for function in parsed_functions {
            entrypoints::write(
                pkg_path,
                function,
                &sources.render_handler_import(&dst.join(pkg_path), function),
                &mut files,
            )?;
        }
        all_functions.extend(parsed_functions.iter().cloned());
    }

    // Update the workspace manifest with all member paths
    workspace_doc["workspace"]["members"] = toml_edit::Array::from_iter(member_paths.iter()).into();

    files.write(Path::new("Cargo.toml"), workspace_doc.to_string())?;

    files.prune_and_save()?;

    let functions = all_functions
        .into_iter()
        .map(|f| {
            let name = f.func_name(false)?;
            let is_deploying = selected_package
                .is_none_or(|package| package.relative_path == f.pkg_rel_path)
                && (deploy_functions.is_empty() || deploy_functions.contains(&name));

            Function::new(project, &f).map(|f| f.set_is_deploying(is_deploying))
        })
        .collect::<eyre::Result<Vec<_>>>()?;

    if let Some(name) = package_name {
        if !functions.iter().any(|function| function.is_deploying) {
            eyre::bail!("No functions match the selection in package `{name}`");
        }
    }

    Ok(functions)
}

/// Read workspace sources, discovering handlers only within the current project.
fn read_packages(project: &Project) -> eyre::Result<BTreeMap<PathBuf, SourcePackage>> {
    project
        .workspace
        .packages
        .iter()
        .map(|package| {
            let directory = project.workspace.root_path.join(&package.relative_path);
            let manifest: toml_edit::DocumentMut =
                fs::read_to_string(directory.join("Cargo.toml"))?.parse()?;
            let lib_path = manifest
                .get("lib")
                .and_then(|lib| lib.get("path"))
                .and_then(toml_edit::Item::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("src/lib.rs"));
            let in_project = project.workspace.is_standalone_crate
                || project.is_ws_root()
                || directory == project.path;
            let parsed = ParsedPackage::read(&directory, &lib_path, in_project.then_some(package))?;
            Ok((
                package.relative_path.clone(),
                SourcePackage {
                    manifest,
                    parsed,
                    lib_path,
                },
            ))
        })
        .collect()
}
