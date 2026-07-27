use super::filehash::{FileHash, CHECKSUMS_FILENAME};
use super::templates;
use super::treeshake::{FunctionSources, TreeShaker};
use super::Project;
use crate::function::Function;
use crate::project::dependencies::insert_lambda_dependency_group;
use eyre::{Context, ContextCompat};
use kinetics_lib::tools::config::EndpointConfig;
use kinetics_parser::{Params, ParsedFunction, Parser, Role};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Clone, Copy)]
enum CopyMode<'a> {
    Exact,
    TreeShaken(&'a FunctionSources),
}

impl<'a> CopyMode<'a> {
    fn function_sources_for(
        self,
        path: &Path,
        relative_path: &Path,
    ) -> Option<&'a FunctionSources> {
        match self {
            Self::TreeShaken(plan)
                if relative_path.starts_with("src")
                    && path.extension().is_some_and(|extension| extension == "rs")
                    && plan.is_module_file(path) =>
            {
                Some(plan)
            }
            Self::Exact | Self::TreeShaken(_) => None,
        }
    }
}

/// Code parsing methods
impl Project {
    /// Parses source code and prepares project for deployment
    ///
    /// Stores rust crate inside target_directory and returns list of encountered functions.
    /// Creates a workspace in the build directory where:
    /// - all existing packages are retained as workspace members unmodified;
    /// - each parsed function populates a new workspace member as a separate build target.
    pub fn parse(
        &self,

        // This method always returns all functions defined in the project, but relies on this input param
        // to mark the requested functions as requested for deployment
        deploy_functions: &[String],
    ) -> eyre::Result<Vec<Function>> {
        let src = &self.workspace.root_path;
        let dst = self.build_path()?;
        // Checksums of source files for preventing rewrite existing files
        let mut checksum = FileHash::new(dst.to_path_buf());

        let mut all_functions = Vec::new();

        // Copy user project into the build folder.
        self.copy_tree(
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
            CopyMode::Exact,
            &mut checksum,
        )?;

        // Build the workspace manifest document.
        // If the project is a single package at the workspace root,
        // create a new manifest with [workspace] section.
        // Otherwise, read and update the existing workspace manifest.
        let mut workspace_doc = if self.workspace.is_standalone_crate {
            toml_edit::DocumentMut::from(toml_edit::Table::from_iter([(
                "workspace",
                toml_edit::Table::new(),
            )]))
        } else {
            fs::read_to_string(src.join("Cargo.toml"))?.parse()?
        };

        // Collect existing member paths for the workspace manifest
        let mut member_paths: Vec<String> = self
            .workspace
            .packages
            .iter()
            .map(|pkg| {
                let path = pkg.relative_path.to_string_lossy();
                if path.is_empty() {
                    pkg.name.clone()
                } else {
                    path.to_string()
                }
            })
            .collect();

        for package in &self.workspace.packages {
            // Copy the initially present workspace member.
            // For standalone crates create a dst workspace member with the same name.
            let pkg_path = if package.relative_path.to_string_lossy().is_empty() {
                &PathBuf::from(&package.name)
            } else {
                &package.relative_path
            };
            self.copy_tree(
                &src.join(&package.relative_path),
                &dst,
                pkg_path,
                &[],
                CopyMode::Exact,
                &mut checksum,
            )?;

            let parsed_functions = Parser::new(&self.workspace.root_path, Some(package))?.functions;
            if parsed_functions.is_empty() {
                continue;
            }

            let shaker = TreeShaker::load(&src.join(&package.relative_path))?;

            for parsed_function in &parsed_functions {
                let function_name = parsed_function.func_name(false)?;

                // Create a new workspace member for this function
                self.create_function_member(
                    src,
                    &dst,
                    &function_name,
                    parsed_function,
                    &shaker,
                    &mut checksum,
                )?;

                member_paths.push(function_name);
            }

            all_functions.extend(parsed_functions);
        }

        // Update the workspace manifest with all member paths
        workspace_doc["workspace"]["members"] =
            toml_edit::Array::from_iter(member_paths.iter()).into();

        let workspace_manifest_path = PathBuf::from("Cargo.toml");
        let manifest_string = workspace_doc.to_string();
        if checksum.update(
            workspace_manifest_path.clone(),
            &FileHash::hash_from_bytes(&manifest_string)
                .wrap_err("Failed to calculate hash from bytes of workspace Cargo.toml")?,
        ) {
            fs::write(dst.join(&workspace_manifest_path), &manifest_string)
                .wrap_err("Failed to write workspace Cargo.toml")?;
        }

        checksum.save().wrap_err("Failed to save checksums")?;
        self.clear_dir(&dst, &checksum)?;

        all_functions
            .into_iter()
            .map(|f| {
                let name = f.func_name(false)?;

                Function::new(self, &f).map(|f| {
                    // Mark function as requested (or not) for deployment
                    f.set_is_deploying(
                        deploy_functions.is_empty() || deploy_functions.contains(&name),
                    )
                })
            })
            .collect::<eyre::Result<Vec<_>>>()
    }

    /// Copy the package dir to a new directory.
    ///
    /// `skip_more` — additional paths (relative to `src`) to skip entirely.
    /// `mode` controls whether Rust module files under `src` are tree-shaken.
    fn copy_tree(
        &self,
        src: &Path,
        dst_dir: &Path,
        dst_rel_path: &Path,
        skip_more: &[PathBuf],
        mode: CopyMode<'_>,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let dst_pkg = dst_dir.join(dst_rel_path);
        fs::create_dir_all(&dst_pkg).wrap_err("Failed to create project copy directory")?;

        let skip_paths = [
            // Skip the target dir, cargo lambda use it (if exist) for incremental builds.
            src.join("target"),
            src.join(".git"),
            src.join(".github"),
        ]
        .into_iter()
        .chain(skip_more.iter().map(|p| src.join(p)))
        .collect::<Vec<_>>();

        let entries = WalkDir::new(src).into_iter().filter_entry(|entry| {
            !skip_paths
                .iter()
                .any(|prefix| entry.path().starts_with(prefix))
        });
        for entry in entries {
            let entry = entry.wrap_err_with(|| format!("Failed to walk source tree {src:?}"))?;
            let src_path = entry.path();

            // Strip leading path from source to create relative path in destination
            let src_relative = src_path.strip_prefix(src).unwrap_or_else(|_| entry.path());
            let dst_path = dst_pkg.join(src_relative);

            if src_path.is_dir() {
                log::debug!("Create dir {dst_path:?}");
                fs::create_dir_all(&dst_path).wrap_err("Create dir failed")?;
                continue;
            }

            let function_sources = mode.function_sources_for(src_path, src_relative);
            if function_sources.is_some_and(|sources| !sources.should_keep(src_path)) {
                log::trace!("pruning unreached file: {src_path:?}");
                continue;
            }

            self.clean_copy(
                src_path,
                dst_dir,
                &dst_rel_path.join(src_relative),
                function_sources,
                checksum,
            )?;
        }

        Ok(())
    }

    /// Remove files that are not present in the source directory
    /// but still exist in the target directory.
    fn clear_dir(&self, dst: &Path, checksum: &FileHash) -> eyre::Result<()> {
        let entries = WalkDir::new(dst).into_iter().filter_entry(|entry| {
            entry
                .path()
                .strip_prefix(dst)
                .is_ok_and(|path| !path.starts_with("target"))
        });
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.wrap_err_with(|| format!("Failed to walk build tree {dst:?}"))?;
            paths.push(entry.into_path());
        }

        for path in paths.into_iter().rev() {
            let Ok(src_relative) = path.strip_prefix(dst) else {
                continue;
            };

            // Leave intact:
            // - the `target` folder;
            // - `.checksums` file.
            // - `Cargo.lock` file.
            if src_relative
                .to_str()
                .is_some_and(|p| p == CHECKSUMS_FILENAME || p == "Cargo.lock")
            {
                continue;
            };

            if path.is_dir() {
                // Delete all folders except those known from file paths in .checksums.
                if !checksum.has_folder(src_relative) {
                    fs::remove_dir_all(&path).wrap_err(format!(
                        "Failed to delete an obsolete folder {src_relative:?}"
                    ))?;
                }
                continue;
            }

            // Delete files not in .checksums.
            if !checksum.has_file(src_relative) {
                fs::remove_file(&path).wrap_err(format!(
                    "failed to delete an obsolete file {src_relative:?}"
                ))?;
            };
        }

        Ok(())
    }

    /// Create lib.rs file for the function crate.
    /// The file is used as an export point for the function.
    fn create_lib(
        &self,
        package_root: &Path,
        dst: &Path,
        dst_pkg_path: &Path,
        function_sources: &FunctionSources,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let src_lib_rs_path = package_root.join("src/lib.rs");
        let lib = function_sources.emit_lib(&src_lib_rs_path)?;

        let relative_lib_path = dst_pkg_path.join("src/lib.rs");
        if checksum.update(
            relative_lib_path.clone(),
            &FileHash::hash_from_bytes(&lib)
                .wrap_err("Failed to calculate hash from bytes of src/lib.rs")?,
        ) {
            fs::write(dst.join(&relative_lib_path), lib).wrap_err("Failed to write src/lib.rs")?;
        }

        Ok(())
    }

    /// Create a new workspace member for a function.
    ///
    /// Each function gets its own package in the workspace with:
    /// - a full copy of the original package's source files;
    /// - `Cargo.toml` based on the original package's manifest with name changed and deps added;
    /// - remote and local lambda bins for the function.
    fn create_function_member(
        &self,
        src: &Path,
        dst: &Path,
        function_name: &str,
        parsed_function: &ParsedFunction,
        shaker: &TreeShaker,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let member_dir = PathBuf::from(function_name);
        let pkg_rel_path = &parsed_function.pkg_rel_path;
        let package_root = src.join(pkg_rel_path);
        let function_sources = shaker.function_sources(parsed_function)?;

        self.copy_tree(
            &package_root,
            dst,
            &member_dir,
            &[PathBuf::from("Cargo.toml"), PathBuf::from("src/lib.rs")],
            CopyMode::TreeShaken(&function_sources),
            checksum,
        )?;
        let lib_name = self.create_function_manifest(
            dst,
            &member_dir,
            function_name,
            parsed_function,
            checksum,
        )?;
        self.create_lib(&package_root, dst, &member_dir, &function_sources, checksum)?;

        let bin_dir = member_dir.join("src/bin");
        fs::create_dir_all(dst.join(&bin_dir)).wrap_err(format!(
            "Failed to create dir for function member {function_name}"
        ))?;

        let function_import = self.import_statement(
            function_sources.module_path(),
            &parsed_function.rust_function_name,
            &lib_name,
        );

        // Create src/bin/<func_name>.rs for the remote and local function
        self.create_lambda_bin(
            dst,
            &bin_dir,
            parsed_function,
            &function_import,
            false,
            checksum,
        )?;
        self.create_lambda_bin(
            dst,
            &bin_dir,
            parsed_function,
            &function_import,
            true,
            checksum,
        )?;

        Ok(())
    }

    /// Create Cargo.toml for a function workspace member.
    ///
    /// Based on the original package's manifest, with:
    /// - package name changed to the function name;
    /// - library target name preserved from the original package;
    /// - lambda runtime dependencies added.
    fn create_function_manifest(
        &self,
        dst: &Path,
        member_dir: &Path,
        function_name: &str,
        parsed_function: &ParsedFunction,
        checksum: &mut FileHash,
    ) -> eyre::Result<String> {
        let manifest_path = member_dir.join("Cargo.toml");
        let src_manifest_path = self
            .workspace
            .root_path
            .join(&parsed_function.pkg_rel_path)
            .join("Cargo.toml");

        let mut doc: toml_edit::DocumentMut = fs::read_to_string(&src_manifest_path)?.parse()?;
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

        doc["package"]["name"] = toml_edit::value(function_name);
        doc.entry("lib")
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut()
            .wrap_err("Invalid [lib] table in Cargo.toml")?
            .insert("name", toml_edit::value(&lib_name));
        self.deps(parsed_function, &mut doc)?;

        let manifest_string = doc.to_string();
        if checksum.update(
            manifest_path.to_path_buf(),
            &FileHash::hash_from_bytes(&manifest_string)
                .wrap_err("Failed to calculate hash from bytes of function Cargo.toml")?,
        ) {
            fs::write(dst.join(&manifest_path), &manifest_string)
                .wrap_err("Failed to write function Cargo.toml")?;
        }

        Ok(lib_name)
    }

    /// Create a function with the code necessary to build lambda
    ///
    /// Set up the function according to cargo lambda guides
    /// within the `bin` folder.
    fn create_lambda_bin(
        &self,
        dst: &Path,
        bin_dir: &Path,
        parsed_function: &ParsedFunction,
        function_import: &str,
        is_local: bool,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let function_name = parsed_function.func_name(is_local)?;
        let lambda_path_local = bin_dir.join(format!("{}.rs", function_name));
        let lambda_path = dst.join(&lambda_path_local);

        let rust_function_name = parsed_function.rust_function_name.clone();
        let main_code = match &parsed_function.params {
            Params::Endpoint(params) => {
                let endpoint_config = EndpointConfig::new(&params.url_path);
                templates::endpoint(
                    function_import,
                    &rust_function_name,
                    endpoint_config,
                    is_local,
                )
            }
            Params::Worker(_) => templates::worker(function_import, &rust_function_name, is_local),
            Params::Cron(_) => templates::cron(function_import, &rust_function_name, is_local),
        };

        let item: syn::File = syn::parse_str(&main_code)?;
        let lambda_content = prettyplease::unparse(&item);
        let content_hash = FileHash::hash_from_bytes(&lambda_content).wrap_err(format!(
            "Failed to calculate hash for bytes of {lambda_path_local:?}"
        ))?;
        if checksum.update(lambda_path_local, &content_hash) {
            fs::write(&lambda_path, &lambda_content)
                .wrap_err(format!("Failed to write {lambda_path:?}"))?;
        }

        Ok(())
    }

    /// Write dependencies required to run a lambda into Cargo.toml
    fn deps(
        &self,
        parsed_function: &ParsedFunction,
        doc: &mut toml_edit::DocumentMut,
    ) -> eyre::Result<()> {
        insert_lambda_dependency_group(doc, "common")?;

        match parsed_function.role {
            Role::Cron | Role::Worker => {
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

    /// Generate the import statement for the function
    /// which is being deployed as a lambda
    fn import_statement(&self, module_path: &[String], rust_name: &str, pkg_name: &str) -> String {
        let pkg_name = pkg_name.replace('-', "_");
        if module_path.is_empty() {
            format!("use {pkg_name}::{rust_name};")
        } else {
            let module_path = module_path.join("::");
            format!("use {pkg_name}::{module_path}::{rust_name};")
        }
    }

    /// Copy a file to the destination folder.
    fn clean_copy(
        &self,
        src: &Path,
        dst_dir: &Path,
        dst_rel_path: &Path,
        function_sources: Option<&FunctionSources>,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let dst_path_full = dst_dir.join(dst_rel_path);
        let content = if let Some(function_sources) = function_sources {
            function_sources.emit_file_content(src)?.into_bytes()
        } else {
            fs::read(src).wrap_err_with(|| format!("Failed to read file {src:?}"))?
        };

        if checksum.update(
            dst_rel_path.to_path_buf(),
            &FileHash::hash_from_bytes(&content)
                .wrap_err_with(|| format!("Failed to calculate hash from bytes of {src:?}"))?,
        ) {
            log::debug!("Copy changed file {dst_path_full:?}");
            fs::write(&dst_path_full, &content)
                .wrap_err_with(|| format!("Failed to write {dst_path_full:?}"))?;
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
