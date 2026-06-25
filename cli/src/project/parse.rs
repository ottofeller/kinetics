use super::filehash::{FileHash, CHECKSUMS_FILENAME};
use super::templates;
use super::Project;
use crate::function::Function;
use crate::project::dependencies::insert_lambda_dependency_group;
use crate::project::treeshake::{PrunedGraph, TreeShakerBuilder};
use eyre::{Context, ContextCompat};
use kinetics::tools::config::EndpointConfig;
use kinetics_parser::{Params, ParsedFunction, Parser, Role};
use regex::Regex;
use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

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
            None,
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
            self.clone(
                &src.join(&package.relative_path),
                &dst,
                pkg_path,
                &[],
                None,
                &mut checksum,
            )?;

            let parsed_functions = Parser::new(&self.workspace.root_path, Some(package))?.functions;
            if parsed_functions.is_empty() {
                continue;
            }

            for parsed_function in &parsed_functions {
                let function_name = parsed_function.func_name(false)?;

                // Create a new workspace member for this function
                self.create_function_member(
                    src,
                    &dst,
                    &function_name,
                    parsed_function,
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

    /// Clone the package dir to a new directory.
    ///
    /// `skip_more` — additional paths (relative to `src`) to skip entirely.
    /// `graph` — module graph that can decide whether to shake the module or retain.
    fn clone(
        &self,
        src: &Path,
        dst_dir: &Path,
        dst_rel_path: &Path,
        skip_more: &[PathBuf],
        graph: Option<&PrunedGraph>,
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

        for entry in WalkDir::new(src)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|entry| {
                let p = entry.path();
                if skip_paths.iter().any(|prefix| p.starts_with(prefix)) {
                    return false;
                }
                // Directories always pass — they'll be created below.
                if p.is_dir() {
                    return true;
                }
                if let Some(graph) = graph {
                    if p.extension().is_some_and(|ext| ext == "rs") && !graph.should_keep(p) {
                        log::debug!("  pruning unreached file: {p:?}");
                        return false;
                    }
                }
                true
            })
        {
            let src_path = entry.path();

            // Strip leading path from source to create relative path in destination
            let src_relative = src_path.strip_prefix(src).unwrap_or_else(|_| entry.path());
            let dst_path = dst_pkg.join(src_relative);

            if src_path.is_dir() {
                log::debug!("Create dir {dst_path:?}");
                fs::create_dir_all(&dst_path).wrap_err("Create dir failed")?;
                continue;
            }

            self.clean_copy(
                src_path,
                dst_dir,
                &dst_rel_path.join(src_relative),
                graph,
                checksum,
            )?;
        }

        Ok(())
    }

    /// Remove files that are not present in the source directory
    /// but still exist in the target directory.
    fn clear_dir(&self, dst: &Path, checksum: &FileHash) -> eyre::Result<()> {
        for entry in WalkDir::new(dst).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();

            let Ok(src_relative) = path.strip_prefix(dst) else {
                continue;
            };

            // Leave intact:
            // - the `target` folder;
            // - `.checksums` file.
            // - `Cargo.lock` file.
            // - non *.rs files
            if src_relative.extension().is_some_and(|ext| ext != "rs")
                || src_relative.strip_prefix("target").is_ok()
                || src_relative
                    .to_str()
                    .is_some_and(|p| p == CHECKSUMS_FILENAME || p == "Cargo.lock")
            {
                continue;
            };

            if path.is_dir() {
                // Delete all folders except those known from file paths in .checksums.
                if !checksum.has_folder(src_relative) {
                    fs::remove_dir_all(path).wrap_err(format!(
                        "Failed to delete an obsolete folder {src_relative:?}"
                    ))?;
                }
                continue;
            }

            // Delete files not in .checksums.
            if !checksum.has_file(src_relative) {
                fs::remove_file(path).wrap_err(format!(
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
        src: &Path,
        src_pkg_path: &Path,
        dst: &Path,
        dst_pkg_path: &Path,
        function: &ParsedFunction,
        graph: Option<&PrunedGraph>,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let src_lib_rs_path = src.join(src_pkg_path).join("src/lib.rs");

        // Take the first path component from function module in the src folder, and export it.
        let fn_path = function.to_string();
        let module = match Path::new(&fn_path)
            .strip_prefix(src_pkg_path)?
            .strip_prefix("src")?
            .with_extension("")
            .components()
            .next()
        {
            Some(Component::Normal(comp)) => comp.to_str().map(String::from),
            _ => None,
        }
        .wrap_err(format!("Invalid path format for {fn_path}"))?;

        let lib = if src_lib_rs_path.exists() {
            // Start from the tree-shaken content (strips orphan mods)
            // and then ensure the target module is exported.
            let lib = if let Some(graph) = graph {
                graph.emit_file_content(&src_lib_rs_path)?
            } else {
                fs::read_to_string(&src_lib_rs_path).wrap_err("Failed to read src/lib.rs")?
            };

            if module != "lib"
                && Regex::new(&format!(r"(?m)^\s*pub\s*mod\s+{module};$"))?
                    .find(&lib)
                    .is_none()
            {
                let re_module = Regex::new(&format!(r"(?m)^\s*mod\s+{module};$"))?;
                let export = format!("pub mod {module};");
                // Delete any existing declaration and append new one
                format!("{export}\n{}", re_module.replace(&lib, ""))
            } else {
                lib
            }
        } else {
            // Create lib.rs file with required exports.
            format!("pub mod {module};\n")
        };

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
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let member_dir = PathBuf::from(function_name);
        let pkg_rel_path = &parsed_function.pkg_rel_path;
        let pkg_abs_path = src.join(pkg_rel_path);

        let shaker = TreeShakerBuilder::new().build(&pkg_abs_path)?;
        let graph = shaker.into_dependency_graph(parsed_function)?;
        let pruned = graph.prune();

        self.clone(
            &pkg_abs_path,
            dst,
            &member_dir,
            &[PathBuf::from("Cargo.toml"), PathBuf::from("src/lib.rs")],
            Some(&pruned),
            checksum,
        )?;

        self.create_function_manifest(dst, &member_dir, function_name, parsed_function, checksum)?;
        self.create_lib(
            src,
            pkg_rel_path,
            dst,
            &member_dir,
            parsed_function,
            Some(&pruned),
            checksum,
        )?;

        let bin_dir = member_dir.join("src/bin");
        fs::create_dir_all(dst.join(&bin_dir)).wrap_err(format!(
            "Failed to create dir for function member {function_name}"
        ))?;

        // Create src/bin/<func_name>.rs for the remote and local function
        self.create_lambda_bin(dst, &bin_dir, parsed_function, false, checksum)?;
        self.create_lambda_bin(dst, &bin_dir, parsed_function, true, checksum)?;

        Ok(())
    }

    /// Create Cargo.toml for a function workspace member.
    ///
    /// Based on the original package's manifest, with:
    /// - package name changed to the function name;
    /// - lambda runtime dependencies added.
    fn create_function_manifest(
        &self,
        dst: &Path,
        member_dir: &Path,
        function_name: &str,
        parsed_function: &ParsedFunction,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let manifest_path = member_dir.join("Cargo.toml");
        let src_manifest_path = self
            .workspace
            .root_path
            .join(&parsed_function.pkg_rel_path)
            .join("Cargo.toml");

        let mut doc: toml_edit::DocumentMut = fs::read_to_string(&src_manifest_path)?.parse()?;
        doc["package"]["name"] = toml_edit::value(function_name);
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

        Ok(())
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
        is_local: bool,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let function_name = parsed_function.func_name(is_local)?;
        let lambda_path_local = bin_dir.join(format!("{}.rs", function_name));
        let lambda_path = dst.join(&lambda_path_local);

        let fn_import = self.import_statement(
            &parsed_function.relative_path,
            &parsed_function.rust_function_name,
            &parsed_function.func_name(false)?,
        )?;

        let rust_function_name = parsed_function.rust_function_name.clone();
        let main_code = match &parsed_function.params {
            Params::Endpoint(params) => {
                let endpoint_config = EndpointConfig::new(&params.url_path);
                templates::endpoint(&fn_import, &rust_function_name, endpoint_config, is_local)
            }
            Params::Worker(_) => templates::worker(&fn_import, &rust_function_name, is_local),
            Params::Cron(_) => templates::cron(&fn_import, &rust_function_name, is_local),
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
        if doc["dependencies"]["kinetics"].as_str().is_some() {
            // Discard string version and write an object
            doc["dependencies"]["kinetics"] =
                toml_edit::Table::from_iter([("version", kinetics_version)]).into();
        } else {
            // For an object overwrite only the version field
            doc["dependencies"]["kinetics"]
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
    fn import_statement(
        &self,
        relative_path: &Path,
        rust_name: &str,
        pkg_name: &str,
    ) -> eyre::Result<String> {
        let relative_path = relative_path.strip_prefix("src/").unwrap_or(relative_path);

        let mut module_path_parts = relative_path
            .components()
            .filter_map(|component| {
                if let std::path::Component::Normal(os_str) = component {
                    os_str.to_str()
                } else {
                    None
                }
            })
            .collect::<Vec<&str>>();

        // Handle lib.rs (root module)
        let is_root_module = relative_path == Path::new("lib.rs");

        let module_path = if is_root_module {
            "".to_string()
        } else {
            // Remove extension from last component
            if let Some(last) = module_path_parts.last_mut() {
                if *last == "mod.rs" {
                    // Remove 'mod.rs'
                    module_path_parts.pop();
                } else {
                    *last = last.trim_end_matches(".rs");
                }
            }
            module_path_parts.join("::")
        };

        let pkg_name = pkg_name.replace('-', "_");
        // If module path is empty then the function is located in the lib.rs file
        let import_statement = if module_path.is_empty() {
            format!("use {pkg_name}::{rust_name};")
        } else {
            format!("use {pkg_name}::{module_path}::{rust_name};")
        };

        Ok(import_statement)
    }

    /// Copy a file to the destination folder.
    fn clean_copy(
        &self,
        src: &Path,
        dst_dir: &Path,
        dst_rel_path: &Path,
        graph: Option<&PrunedGraph>,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let dst_path_full = dst_dir.join(dst_rel_path);
        // For all non .rs files just copy it.
        if src.extension().is_some_and(|ext| ext != "rs") {
            log::debug!("Copy without checksum {dst_path_full:?}");
            return fs::copy(src, &dst_path_full)
                .wrap_err_with(|| format!("Failed to copy file {src:?} -> {dst_path_full:?}"))
                .map(|_| ());
        }

        // Update hash table for the file.
        let content = if let Some(graph) = graph {
            graph.emit_file_content(src)?
        } else {
            fs::read_to_string(src).wrap_err(format!("Failed to read file {src:?}"))?
        };
        if checksum.update(
            dst_rel_path.to_path_buf(),
            &FileHash::hash_from_bytes(&content)
                .wrap_err_with(|| format!("Failed to calculate hash from bytes of {src:?}"))?,
        ) {
            log::debug!("Copy with changed checksum {dst_path_full:?}");
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
