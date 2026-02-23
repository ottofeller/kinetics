use super::filehash::{FileHash, CHECKSUMS_FILENAME};
use super::templates;
use super::Project;
use crate::function::Function;
use crate::tools::config::EndpointConfig;
use eyre::Context;
use kinetics_parser::{ParsedFunction, Parser, Role};
use regex::Regex;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use walkdir::WalkDir;

/// Code parsing methods
impl Project {
    /// Parses source code and prepares project for deployment
    ///
    /// Stores rust crate inside target_directory and returns list of encountered functions
    pub fn parse(
        &self,
        dst: PathBuf,

        // This method always returns all functions defined in the project, but relies on this input param
        // to mark the requested functions as requested for deployment
        deploy_functions: &[String],
    ) -> eyre::Result<Vec<Function>> {
        // Parse functions from source code
        let parsed_functions = Parser::new(Some(&self.path))?.functions;
        let src = &self.path;
        let dst = dst.join(&self.name);
        // Checksums of source files for preventing rewrite existing files
        let mut checksum = FileHash::new(dst.to_path_buf());

        // Clone user project into the build folder.
        self.clone(src, &dst, &mut checksum)?;

        // Create lib.rs exporting a containing module of each parsed function.
        self.create_lib(src, &dst, &parsed_functions, &mut checksum)?;

        let relative_manifest_path = Path::new("Cargo.toml");
        let mut manifest: toml_edit::DocumentMut =
            fs::read_to_string(src.join(relative_manifest_path))?.parse()?;
        let bin_dir = Path::new("src/bin");
        fs::create_dir_all(dst.join(bin_dir)).wrap_err("Create dir failed")?;

        for parsed_function in &parsed_functions {
            for is_local in [false, true] {
                // Create bin file for every parsed function
                self.create_lambda_bin(&dst, bin_dir, parsed_function, is_local, &mut checksum)?;

                // Add dependencies required to run a lambda to Cargo.toml
                self.deps(parsed_function, is_local, &mut manifest)?;
            }
        }

        let manifest_string = manifest.to_string();
        if checksum.update(
            relative_manifest_path.to_path_buf(),
            &FileHash::hash_from_bytes(&manifest_string)
                .wrap_err("Failed to calculate hash from bytes of Cargo.toml")?,
        ) {
            fs::write(dst.join(relative_manifest_path), &manifest_string)
                .wrap_err("Failed to write Cargo.toml")?;
        }

        checksum.save().wrap_err("Failed to save checksums")?;
        self.clear_dir(&dst, &checksum)?;

        // Create a new project instance for the target build directory
        let dst_project = Project::from_path(dst.to_path_buf())?;

        parsed_functions
            .into_iter()
            .map(|f| {
                let name = f.func_name(false)?;

                Function::new(&dst_project, &f).map(|f| {
                    // Mark function as requested (or not) for deployment
                    f.set_is_deploying(
                        deploy_functions.is_empty() || deploy_functions.contains(&name),
                    )
                })
            })
            .collect::<eyre::Result<Vec<_>>>()
    }

    /// Clone the project dir to a new directory
    fn clone(&self, src: &Path, dst: &Path, checksum: &mut FileHash) -> eyre::Result<()> {
        fs::create_dir_all(dst).wrap_err("Failed to create dir to clone the project to")?;

        let skip_paths = [
            // Skip the target dir, cargo lambda use it (if exist) for incremental builds.
            src.join("target"),
            src.join(".git"),
            src.join(".github"),
            // Skip project manifest, since we process it later.
            src.join("Cargo.toml"),
        ];

        for entry in WalkDir::new(src)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|entry| {
                !skip_paths
                    .iter()
                    .any(|prefix| entry.path().starts_with(prefix))
            })
        {
            let src_path = entry.path();

            // Strip leading path from source to create relative path in destination
            let src_relative = entry
                .path()
                .strip_prefix(src)
                .unwrap_or_else(|_| entry.path());

            let dst_path = dst.join(src_relative);

            if src_path.is_dir() {
                fs::create_dir_all(&dst_path).wrap_err("Create dir failed")?;
                continue;
            }

            // If src file has been modified, copy it to the destination
            self.clean_copy(src_path, dst, src_relative, checksum)?;
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
            if src_relative.strip_prefix("target").is_ok()
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

    /// Create lib.rs file for the cloned crate.
    /// The file is used as an export point for all the functions.
    fn create_lib(
        &self,
        src: &Path,
        dst: &Path,
        functions: &[ParsedFunction],
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let relative_lib_path = Path::new("src").join("lib.rs");
        let src_lib_rs_path = src.join(&relative_lib_path);

        let modules = functions.iter().filter_map(|f| {
            // Take the first path component from each module in the src folder, and export it.
            match Path::new(&f.relative_path)
                .strip_prefix("src")
                .ok()?
                .with_extension("")
                .components()
                .next()
            {
                Some(Component::Normal(comp)) => comp.to_str().map(String::from),
                _ => None,
            }
        });

        let lib = if src_lib_rs_path.exists() {
            // Make sure all modules with functions are exported.
            let mut lib =
                fs::read_to_string(src_lib_rs_path).wrap_err("Failed to read src/lib.rs")?;

            for module in modules {
                if module != "lib" {
                    let re_module_pub = Regex::new(&format!(r"(?m)^\s*pub\s*mod\s+{module};$"))?;
                    if re_module_pub.find(&lib).is_some() {
                        // Leave already public modules as is.
                        continue;
                    };

                    let re_module = Regex::new(&format!(r"(?m)^\s*mod\s+{module};$"))?;
                    let export = format!("pub mod {module};");
                    // Delete any existing declaration and append new one
                    lib = format!("{export}\n{}", re_module.replace(&lib, ""));
                }
            }

            lib
        } else {
            // Create lib.rs file with required exports.
            modules
                .map(|module| format!("pub mod {module};\n"))
                .collect::<String>()
        };

        if checksum.update(
            relative_lib_path.to_path_buf(),
            &FileHash::hash_from_bytes(&lib)
                .wrap_err("Failed to calculate hash from bytes of src/lib.rs")?,
        ) {
            fs::write(dst.join(&relative_lib_path), lib).wrap_err("Failed to write src/lib.rs")?;
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
            &self.name,
        )?;

        let rust_function_name = parsed_function.rust_function_name.clone();
        let main_code = match &parsed_function.role {
            Role::Endpoint(params) => {
                let endpoint_config = EndpointConfig::new(&params.url_path);
                templates::endpoint(&fn_import, &rust_function_name, endpoint_config, is_local)
            }
            Role::Worker(_) => templates::worker(&fn_import, &rust_function_name, is_local),
            Role::Cron(_) => templates::cron(&fn_import, &rust_function_name, is_local),
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
        is_local: bool,
        doc: &mut toml_edit::DocumentMut,
    ) -> eyre::Result<()> {
        if matches!(parsed_function.role, Role::Cron(_) | Role::Worker(_))
            || (matches!(parsed_function.role, Role::Endpoint(_)) && is_local)
        {
            if let Some(serde_json) = doc["dependencies"]["serde_json"]
                .or_insert(toml_edit::Table::new().into())
                .as_table_mut()
            {
                serde_json.insert("version", toml_edit::value("1.0.149"));
            }

            if let Some(reqwest) = doc["dependencies"]["reqwest"]
                .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                .as_table_mut()
            {
                reqwest.insert("version", toml_edit::value("0.13.1"));
                reqwest.insert("default-features", toml_edit::value(false));
                reqwest.insert(
                    "features",
                    toml_edit::Array::from_iter(["default-tls"]).into(),
                );
            }
        }

        match parsed_function.role {
            Role::Cron(_) | Role::Worker(_) => {
                doc["dependencies"]["lambda_runtime"]
                    .or_insert(toml_edit::Table::new().into())
                    .as_table_mut()
                    .map(|t| t.insert("version", toml_edit::value("^1.0")));
            }
            Role::Endpoint(_) => {
                doc["dependencies"]["lambda_http"]
                    .or_insert(toml_edit::Table::new().into())
                    .as_table_mut()
                    .map(|t| t.insert("version", toml_edit::value("^1.0")));
                doc["dependencies"]["http"]
                    .or_insert(toml_edit::Table::new().into())
                    .as_table_mut()
                    .map(|t| t.insert("version", toml_edit::value("^1.0")));
                doc["dependencies"]["tower"]
                    .or_insert(toml_edit::Table::new().into())
                    .as_table_mut()
                    .map(|t| t.insert("version", toml_edit::value("^0")));
            }
        };

        doc["dependencies"]["kinetics"]
            .or_insert(toml_edit::Table::new().into())
            .as_table_mut()
            .map(|t| t.insert("version", toml_edit::value("0.11.7")));

        doc["dependencies"]["aws_lambda_events"]
            .or_insert(toml_edit::Table::new().into())
            .as_table_mut()
            .map(|t| t.insert("version", toml_edit::value("0.18.0")));

        doc["dependencies"]["aws-config"]
            .or_insert(toml_edit::Table::new().into())
            .as_table_mut()
            .map(|t| t.insert("version", toml_edit::value("1.8.12")));

        doc["dependencies"]["aws-sdk-ssm"]
            .or_insert(toml_edit::Table::new().into())
            .as_table_mut()
            .map(|t| t.insert("version", toml_edit::value("1.59.0")));

        doc["dependencies"]["aws-sdk-sqs"]
            .or_insert(toml_edit::Table::new().into())
            .as_table_mut()
            .map(|t| t.insert("version", toml_edit::value("1.91.0")));

        if let Some(tokio_dep) = doc["dependencies"]["tokio"]
            .or_insert(toml_edit::Table::new().into())
            .as_table_mut()
        {
            tokio_dep.insert("version", toml_edit::value("1.49.0"));
            tokio_dep.insert("features", toml_edit::Array::from_iter(["full"]).into());
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
        relative_path: &str,
        rust_name: &str,
        project_name: &str,
    ) -> eyre::Result<String> {
        let relative_path =
            PathBuf::from_str(relative_path.strip_prefix("src/").unwrap_or(relative_path))?;

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

        // If module path is empty then the function is located in the lib.rs file
        let import_statement = if module_path.is_empty() {
            format!("use {project_name}::{rust_name};")
        } else {
            format!("use {project_name}::{module_path}::{rust_name};")
        };

        Ok(import_statement)
    }

    /// Delete the macro attributes from a file
    /// and copy it to the destination folder.
    fn clean_copy(
        &self,
        src_path_full: &Path,
        dst_dir: &Path,
        file_path_relative: &Path,
        checksum: &mut FileHash,
    ) -> eyre::Result<()> {
        let dst_path_full = dst_dir.join(file_path_relative);
        // For all non .rs files just copy it.
        if src_path_full.extension().is_some_and(|ext| ext != "rs") {
            return fs::copy(src_path_full, &dst_path_full)
                .wrap_err_with(|| {
                    format!("Failed to copy file {src_path_full:?} -> {dst_path_full:?}")
                })
                .map(|_| ());
        }

        // Attempt kinetics macro replacements in .rs files.
        let content = &fs::read_to_string(src_path_full)
            .wrap_err(format!("Failed to read file {src_path_full:?}"))?;
        if checksum.update(
            file_path_relative.to_path_buf(),
            &FileHash::hash_from_bytes(&content).wrap_err_with(|| {
                format!("Failed to calculate hash from bytes of {src_path_full:?}")
            })?,
        ) {
            fs::write(&dst_path_full, &content)
                .wrap_err_with(|| format!("Failed to write {dst_path_full:?}"))?;
        }

        Ok(())
    }
}
