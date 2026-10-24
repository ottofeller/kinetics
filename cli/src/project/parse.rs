use super::filehash::{FileHash, CHECKSUMS_FILENAME};
use super::prepare::PreparedPackage;
use super::templates;
use super::Project;
use crate::function::Function;
use crate::project::dependencies::insert_lambda_dependency_group;
use eyre::{Context, ContextCompat};
use kinetics_lib::tools::config::EndpointConfig;
use kinetics_parser::{Params, ParsedFunction, ParsedPackage, Parser, Role};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

struct SourcePackage {
    manifest: toml_edit::DocumentMut,
    parsed: ParsedPackage,
    lib_path: PathBuf,
}

/// Code parsing methods
impl Project {
    /// Parses source code and prepares project for deployment
    ///
    /// Stores rust crate inside target_directory and returns list of encountered functions.
    /// Creates a workspace in the build directory where:
    /// - each source package provides one shared library;
    /// - each parsed function gets remote and local binary targets in its source package.
    pub fn parse(
        &self,

        // This method always returns all functions defined in the project, but relies on this input param
        // to mark the requested functions as requested for deployment
        deploy_functions: &[String],
    ) -> eyre::Result<Vec<Function>> {
        self.parse_with_package(deploy_functions, None)
    }

    /// Prepare every project function, selecting a package and function names for deployment.
    pub(crate) fn parse_with_package(
        &self,
        deploy_functions: &[String],
        package_name: Option<&str>,
    ) -> eyre::Result<Vec<Function>> {
        let selected_package = package_name
            .map(|name| self.workspace.package(name))
            .transpose()?;

        if let (Some(package), Some(name)) = (selected_package, package_name) {
            if !self.workspace.is_standalone_crate
                && self.path != self.workspace.root_path
                && self.workspace.root_path.join(&package.relative_path) != self.path
            {
                eyre::bail!(
                    "Package `{name}` is outside the current Kinetics project; use --project to select its project",
                );
            }
        }

        let src = &self.workspace.root_path;
        let dst = self.build_path()?;
        // Checksums of source files for preventing rewrite existing files
        let mut checksum = FileHash::new(dst.to_path_buf());

        let packages = self.read_packages()?;
        let mut all_functions = Vec::new();

        // Clone user project into the build folder.
        self.clone(
            src,
            &dst,
            &PathBuf::new(),
            // Ignore all workspace members - we copy them later
            &self
                .workspace
                .packages
                .iter()
                .map(|pkg| pkg.relative_path.clone())
                // Skip the root Cargo.toml, since we populate it ourselves.
                .chain([PathBuf::from("Cargo.toml")])
                .collect::<Vec<_>>(),
            &mut checksum,
        )?;

        let mut workspace_doc: toml_edit::DocumentMut =
            fs::read_to_string(src.join("Cargo.toml"))?.parse()?;

        // Collect existing member paths for the workspace manifest
        let member_paths: Vec<String> = self
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

        for package in &self.workspace.packages {
            let pkg_path = &package.relative_path;
            let source = &packages[pkg_path];
            let parsed_functions = &source.parsed.functions;
            let mut package_doc = source.manifest.clone();
            let prepared = if parsed_functions.is_empty() {
                None
            } else {
                let lib_name = self.prepare_package_manifest(&mut package_doc, parsed_functions)?;
                let references = self
                    .workspace
                    .references
                    .iter()
                    .filter(|reference| {
                        reference.package == *pkg_path && reference.source != *pkg_path
                    })
                    .map(|reference| (&packages[&reference.source].parsed, reference.name.as_str()))
                    .collect::<Vec<_>>();
                Some(PreparedPackage::new(
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
            if let Some(prepared) = &prepared {
                skip_paths.extend(prepared.shared.keys().cloned());
            }
            if pkg_path.as_os_str().is_empty() {
                skip_paths.extend(
                    self.workspace
                        .packages
                        .iter()
                        .filter(|pkg| !pkg.relative_path.as_os_str().is_empty())
                        .map(|pkg| pkg.relative_path.clone()),
                );
            }

            self.clone(
                &src.join(pkg_path),
                &dst,
                pkg_path,
                &skip_paths,
                &mut checksum,
            )?;

            let Some(prepared) = prepared else { continue };
            for (path, syntax) in &prepared.shared {
                checksum.write(&pkg_path.join(path), prettyplease::unparse(syntax))?;
            }
            for (path, syntax) in &prepared.handlers {
                checksum.write(
                    &pkg_path.join(".kinetics/handlers").join(path),
                    prettyplease::unparse(syntax),
                )?;
            }
            if pkg_path.as_os_str().is_empty() {
                workspace_doc = package_doc;
            } else {
                checksum.write(&pkg_path.join("Cargo.toml"), package_doc.to_string())?;
            }
            for function in parsed_functions {
                self.create_function_bins(&dst, pkg_path, function, &prepared, &mut checksum)?;
            }
            all_functions.extend(parsed_functions.iter().cloned());
        }

        // Update the workspace manifest with all member paths
        workspace_doc["workspace"]["members"] =
            toml_edit::Array::from_iter(member_paths.iter()).into();

        checksum.write(Path::new("Cargo.toml"), workspace_doc.to_string())?;

        self.clear_dir(&dst, &checksum)?;
        checksum.save().wrap_err("Failed to save checksums")?;

        let functions = all_functions
            .into_iter()
            .map(|f| {
                let name = f.func_name(false)?;
                let is_deploying = selected_package
                    .is_none_or(|package| package.relative_path == f.pkg_rel_path)
                    && (deploy_functions.is_empty() || deploy_functions.contains(&name));

                Function::new(self, &f).map(|f| f.set_is_deploying(is_deploying))
            })
            .collect::<eyre::Result<Vec<_>>>()?;

        if let Some(name) = package_name {
            if !functions.iter().any(|function| function.is_deploying) {
                eyre::bail!("No functions match the selection in package `{name}`");
            }
        }

        Ok(functions)
    }

    fn read_packages(&self) -> eyre::Result<BTreeMap<PathBuf, SourcePackage>> {
        self.workspace
            .packages
            .iter()
            .map(|package| {
                let directory = self.workspace.root_path.join(&package.relative_path);
                let manifest: toml_edit::DocumentMut =
                    fs::read_to_string(directory.join("Cargo.toml"))?.parse()?;
                let lib_path = manifest
                    .get("lib")
                    .and_then(|lib| lib.get("path"))
                    .and_then(toml_edit::Item::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("src/lib.rs"));
                let in_project = self.workspace.is_standalone_crate
                    || self.is_ws_root()
                    || directory == self.path;
                let parsed =
                    ParsedPackage::read(&directory, &lib_path, in_project.then_some(package))?;
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

    /// Clone the package dir to a new directory
    fn clone(
        &self,
        src: &Path,
        dst_dir: &Path,
        dst_rel_path: &Path,
        skip_more: &[PathBuf],
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let dst_pkg = dst_dir.join(dst_rel_path);
        fs::create_dir_all(&dst_pkg).wrap_err("Failed to create dir to clone the project to")?;

        let skip_paths = [
            // Skip the target dir, cargo lambda use it (if exist) for incremental builds.
            src.join("target"),
            src.join(".git"),
            src.join(".github"),
        ]
        .into_iter()
        .chain(skip_more.iter().map(|p| src.join(p)))
        .collect::<Vec<_>>();

        for entry in WalkDir::new(src).into_iter().filter_entry(|entry| {
            !skip_paths
                .iter()
                .any(|prefix| entry.path().starts_with(prefix))
        }) {
            let entry =
                entry.wrap_err_with(|| format!("Failed to read source directory {src:?}"))?;
            let src_path = entry.path();

            // Strip leading path from source to create relative path in destination
            let src_relative = src_path.strip_prefix(src).unwrap_or_else(|_| entry.path());
            let dst_path = dst_pkg.join(src_relative);

            if src_path.is_dir() {
                log::debug!("Create dir {dst_path:?}");
                fs::create_dir_all(&dst_path).wrap_err("Create dir failed")?;
                continue;
            }

            // If src file has been modified, copy it to the destination
            self.clean_copy(
                src_path,
                dst_dir,
                &dst_rel_path.join(src_relative),
                checksum,
            )?;
        }

        Ok(())
    }

    /// Remove files that are not present in the source directory
    /// but still exist in the target directory.
    fn clear_dir(&self, dst: &Path, checksum: &FileHash) -> eyre::Result<()> {
        let mut entries = WalkDir::new(dst).into_iter();
        while let Some(entry) = entries.next() {
            let entry =
                entry.wrap_err_with(|| format!("Failed to read build directory {dst:?}"))?;
            let path = entry.path();

            let Ok(src_relative) = path.strip_prefix(dst) else {
                continue;
            };

            // Leave intact:
            // - the build directory itself;
            // - the `target` folder;
            // - `.checksums` file.
            // - `Cargo.lock` file.
            if src_relative.as_os_str().is_empty() {
                continue;
            }
            if src_relative.strip_prefix("target").is_ok()
                || src_relative
                    .to_str()
                    .is_some_and(|p| p == CHECKSUMS_FILENAME || p == "Cargo.lock")
            {
                if entry.file_type().is_dir() {
                    entries.skip_current_dir();
                }
                continue;
            };

            if entry.file_type().is_dir() {
                // Delete folders with no files registered during this parse.
                if !checksum.has_folder(src_relative) {
                    entries.skip_current_dir();
                    fs::remove_dir_all(path).wrap_err(format!(
                        "Failed to delete an obsolete folder {src_relative:?}"
                    ))?;
                }
                continue;
            }

            // Delete files not registered during this parse.
            if !checksum.has_file(src_relative) {
                fs::remove_file(path).wrap_err(format!(
                    "failed to delete an obsolete file {src_relative:?}"
                ))?;
            };
        }

        Ok(())
    }

    /// Create remote and local entry points from the prepared package.
    fn create_function_bins(
        &self,
        dst: &Path,
        pkg_path: &Path,
        function: &ParsedFunction,
        prepared: &PreparedPackage,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let bin_dir = pkg_path.join("src/bin");
        let import = prepared.binary_import(&dst.join(pkg_path), function);
        self.create_lambda_bin(&bin_dir, function, &import, false, checksum)?;
        self.create_lambda_bin(&bin_dir, function, &import, true, checksum)
    }

    fn prepare_package_manifest(
        &self,
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
        for function in functions {
            self.deps(function, doc)?;
        }

        Ok(lib_name)
    }

    /// Create a function with the code necessary to build lambda
    ///
    /// Set up the function according to cargo lambda guides
    /// within the `bin` folder.
    fn create_lambda_bin(
        &self,
        bin_dir: &Path,
        parsed_function: &ParsedFunction,
        fn_import: &str,
        is_local: bool,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let function_name = parsed_function.func_name(is_local)?;
        let lambda_path_local = bin_dir.join(format!("{}.rs", function_name));

        let rust_function_name = "__kinetics_user_function";
        let main_code = match &parsed_function.params {
            Params::Endpoint(params) => {
                let endpoint_config = EndpointConfig::new(&params.url_path);
                templates::endpoint(fn_import, rust_function_name, endpoint_config, is_local)
            }
            Params::Worker(_) => templates::worker(fn_import, rust_function_name, is_local),
            Params::DlqWorker(_) => templates::worker(fn_import, rust_function_name, is_local),
            Params::Cron(_) => templates::cron(fn_import, rust_function_name, is_local),
        };

        let item: syn::File = syn::parse_str(&main_code)?;
        let lambda_content = prettyplease::unparse(&item);
        checksum.write(&lambda_path_local, lambda_content)
    }

    /// Write dependencies required to run a lambda into Cargo.toml
    fn deps(
        &self,
        parsed_function: &ParsedFunction,
        doc: &mut toml_edit::DocumentMut,
    ) -> eyre::Result<()> {
        insert_lambda_dependency_group(doc, "common")?;

        match parsed_function.role {
            Role::Cron | Role::Worker | Role::DlqWorker => {
                insert_lambda_dependency_group(doc, "cron")?;
            }
            Role::Endpoint => {
                insert_lambda_dependency_group(doc, "endpoint")?;
            }
        };

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

    /// Copy a file only when its contents changed or the destination is missing.
    fn clean_copy(
        &self,
        src: &Path,
        dst_dir: &Path,
        dst_rel_path: &Path,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let dst_path_full = dst_dir.join(dst_rel_path);
        if checksum.update(dst_rel_path.to_path_buf(), &FileHash::hash_from_file(src)?) {
            log::debug!("Copy with changed checksum {dst_path_full:?}");
            fs::copy(src, &dst_path_full)
                .wrap_err_with(|| format!("Failed to copy file {src:?} -> {dst_path_full:?}"))?;
        }

        Ok(())
    }

    pub fn functions(&self) -> eyre::Result<Vec<Function>> {
        self.workspace
            .packages
            .iter()
            .map(|pkg| Parser::new(&self.workspace.root_path, Some(pkg)))
            .try_fold(Vec::new(), |mut acc, parser| {
                for f in parser?.functions {
                    acc.push(Function::new(self, &f)?);
                }
                Ok(acc)
            })
    }

    pub fn parsed_functions(&self) -> eyre::Result<Vec<ParsedFunction>> {
        self.workspace
            .packages
            .iter()
            .map(|pkg| Parser::new(&self.workspace.root_path, Some(pkg)))
            .try_fold(Vec::new(), |mut acc, parser| {
                acc.append(&mut parser?.functions);
                Ok(acc)
            })
    }
}
