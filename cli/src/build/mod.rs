mod filehash;
pub mod pipeline;
mod templates;
use crate::build::pipeline::Pipeline;
use crate::crat::Crate;
use crate::function::Function;
use eyre::{Context, OptionExt};
use filehash::{FileHash, CHECKSUMS_FILENAME};
use kinetics_parser::{ParsedFunction, Parser, Role};
use regex::Regex;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use walkdir::WalkDir;

/// The entry point to run the command
pub async fn run(deploy_functions: &[String]) -> eyre::Result<()> {
    Pipeline::builder()
        .with_deploy_enabled(false)
        .set_crat(Crate::from_current_dir()?)
        .build()
        .wrap_err("Failed to build pipeline")?
        .run(deploy_functions)
        .await?;

    Ok(())
}

/// Parses source code and prepares crates for deployment
/// Stores crates inside target_directory and returns list of encountered functions
pub fn prepare_crates(
    dst: PathBuf,
    current_crate: &Crate,

    // prepare_crates() always returns all functions defined in the crate, but relies on this input param
    // to mark the requested functions as requested for deployment
    deploy_functions: &[String],
) -> eyre::Result<Vec<Function>> {
    // Parse functions from source code
    let parsed_functions = Parser::new(Some(&current_crate.path))?.functions;

    let src = &current_crate.path;
    let dst = dst.join(&current_crate.name);
    // Checksums of source files for preventing rewrite existing files
    let mut checksum = FileHash::new(dst.to_path_buf());

    // Clone user project into the build folder.
    clone(src, &dst, &mut checksum)?;

    // Create lib.rs exporting a containing module of each parsed function.
    create_lib(src, &dst, &parsed_functions, &mut checksum)?;

    let relative_manifest_path = Path::new("Cargo.toml");
    let mut manifest: toml_edit::DocumentMut =
        fs::read_to_string(src.join(relative_manifest_path))?.parse()?;
    let bin_dir = Path::new("src/bin");
    fs::create_dir_all(dst.join(bin_dir)).wrap_err("Create dir failed")?;

    for parsed_function in &parsed_functions {
        for is_local in [false, true] {
            // Create bin file for every parsed function
            create_lambda_bin(
                &dst,
                bin_dir,
                parsed_function,
                is_local,
                &mut manifest,
                &mut checksum,
            )?;

            // Fill in necessary data in Cargo.toml
            metadata(parsed_function, is_local, &mut manifest)?;
            deps(parsed_function, is_local, &mut manifest)?;
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
    clear_dir(&dst, &checksum)?;

    parsed_functions
        .into_iter()
        .map(|f| {
            let name = f.func_name(false);
            Function::new(&dst, &name).map(|f| {
                f.is_deploying(deploy_functions.is_empty() || deploy_functions.contains(&name))
            })
        })
        .collect::<eyre::Result<Vec<_>>>()
}

/// Clone the crate dir to a new directory
fn clone(src: &Path, dst: &Path, checksum: &mut FileHash) -> eyre::Result<()> {
    fs::create_dir_all(dst).wrap_err("Failed to create dir to clone the crate to")?;

    // Skip source target from copying
    let src_target = src.join("target");
    // Handle Cargo.toml as a special case
    let relative_cargo_path = Path::new("Cargo.toml");
    let src_cargo_path = src.join(relative_cargo_path);

    for entry in WalkDir::new(src)
        .into_iter()
        .filter_map(|e| e.ok())
        // Skip the target dir, cargo lambda use it (if exist) for incremental builds.
        .filter(|entry| !entry.path().starts_with(&src_target))
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

        // Cargo.toml found
        if src_relative == relative_cargo_path {
            // Copy Cargo.toml with modifications
            let mut cargo_toml: toml_edit::DocumentMut =
                fs::read_to_string(&src_cargo_path)?.parse()?;
            if let Some(deps_table) = cargo_toml["dependencies"].as_table_mut() {
                deps_table.remove("kinetics-macro");
            }
            let cargo_toml_string = cargo_toml.to_string();
            if checksum.update(
                relative_cargo_path.to_path_buf(),
                &FileHash::hash_from_bytes(&cargo_toml_string)
                    .wrap_err("Failed to calculate hash from bytes of Cargo.toml")?,
            ) {
                fs::write(&dst_path, &cargo_toml_string).wrap_err("Failed to write Cargo.toml")?;
            }
            continue;
        }

        // If src file has been modified, copy it to the destination
        clean_copy(src_path, dst, src_relative, checksum)?;
    }

    Ok(())
}

/// Remove files that are not present in the source directory
/// but still exist in the target directory.
fn clear_dir(dst: &Path, checksum: &FileHash) -> eyre::Result<()> {
    for entry in WalkDir::new(dst).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        let Ok(src_relative) = path.strip_prefix(dst) else {
            continue;
        };

        // Leave intact:
        // - the `target` folder;
        // - `.checksums` file.
        if src_relative.strip_prefix("target").is_ok()
            || src_relative
                .to_str()
                .is_some_and(|p| p == CHECKSUMS_FILENAME)
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

/// Attempt kinetics macro replacements in .rs files.
fn remove_kinetics_macro(content: &str) -> eyre::Result<String> {
    let re_endpoint = Regex::new(r"(?m)^\s*#\s*\[\s*endpoint[^]]*]\s*$")?;
    let re_cron = Regex::new(r"(?m)^\s*#\s*\[\s*cron[^]]*]\s*$")?;
    let re_worker = Regex::new(r"(?m)^\s*#\s*\[\s*worker[^]]*]\s*$")?;

    let re_import = Regex::new(
        r"(?m)^\s*use\s+kinetics_macro(\s*::\s*(\w+|\{\s*\w+(\s*,\s*\w+)*\s*}))?\s*;\s*$",
    )?;

    let mut new_content = re_endpoint.replace_all(content, "").to_string();
    new_content = re_worker.replace_all(&new_content, "").to_string();
    new_content = re_cron.replace_all(&new_content, "").to_string();
    Ok(re_import.replace_all(&new_content, "").into_owned())
}

/// Create lib.rs file for the cloned crate.
/// The file is used as an export point for all the functions.
fn create_lib(
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
        let mut lib = fs::read_to_string(src_lib_rs_path).wrap_err("Failed to read src/lib.rs")?;

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

        remove_kinetics_macro(&lib)?
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

/// Create crate with the code necessary to build lambda
///
/// Set up the function according to cargo lambda guides
/// within the `bin` folder.
fn create_lambda_bin(
    dst: &Path,
    bin_dir: &Path,
    parsed_function: &ParsedFunction,
    is_local: bool,
    manifest: &mut toml_edit::DocumentMut,
    checksum: &mut FileHash,
) -> eyre::Result<()> {
    let function_name = parsed_function.func_name(is_local);
    let lambda_path_local = bin_dir.join(format!("{}.rs", function_name));
    let lambda_path = dst.join(&lambda_path_local);

    let fn_import = import_statement(
        &parsed_function.relative_path,
        &parsed_function.rust_function_name,
        manifest
            .get("package")
            .ok_or_eyre("No [package]")?
            .get("name")
            .ok_or_eyre("No [name]")?
            .as_str()
            .ok_or_eyre("Not a string [name]")?,
    )?;

    let rust_function_name = parsed_function.rust_function_name.clone();
    let main_code = match &parsed_function.role {
        Role::Endpoint(_) => templates::endpoint(&fn_import, &rust_function_name, is_local),
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

/// Write function metadata into the project Cargo.toml
fn metadata(
    parsed_function: &ParsedFunction,
    is_local: bool,
    manifest: &mut toml_edit::DocumentMut,
) -> eyre::Result<()> {
    manifest["package"]["metadata"].or_insert(toml_edit::Table::new().into())["kinetics"]
        .or_insert(toml_edit::Table::new().into())["functions"]
        .or_insert(toml_edit::ArrayOfTables::new().into());
    let functions_meta = manifest["package"]["metadata"]["kinetics"]["functions"]
        .as_array_of_tables_mut()
        .expect("[functions] array was created above");

    let role_str = match &parsed_function.role {
        Role::Endpoint(_) => "endpoint",
        Role::Cron(_) => "cron",
        Role::Worker(_) => "worker",
    };

    let name = parsed_function.func_name(is_local);

    // Create a function table for both roles
    let mut function_table = toml_edit::Table::new();
    function_table["name"] = toml_edit::value(&name);
    function_table["role"] = toml_edit::value(role_str);
    function_table["is_local"] = toml_edit::value(is_local);
    let mut function_meta = toml_edit::Table::new();
    function_meta.insert("function", function_table.into());

    match &parsed_function.role {
        Role::Worker(params) => {
            let mut queue_table = toml_edit::Table::new();
            queue_table["name"] = toml_edit::value(&name);
            if let Some(queue_alias) = &params.queue_alias {
                queue_table["alias"] = toml_edit::value(queue_alias);
            };
            queue_table["concurrency"] = toml_edit::value(params.concurrency as i64);
            queue_table["fifo"] = toml_edit::value(params.fifo);

            let mut named_table = toml_edit::Table::new();
            named_table.set_implicit(true); // Don't create an empty queue table
            named_table.insert(&name, queue_table.into());

            function_meta.insert("queue", named_table.into());
        }
        Role::Endpoint(params) => {
            let mut endpoint_table = toml_edit::Table::new();
            if let Some(url_path) = &params.url_path {
                endpoint_table["url_path"] = toml_edit::value(url_path);
            }

            endpoint_table["queues"] = toml_edit::value(
                serde_json::to_string(&params.queues.clone().unwrap_or_default()).unwrap(),
            );

            if params.is_disabled.is_some() {
                endpoint_table["is_disabled"] = toml_edit::value(params.is_disabled.unwrap());
            }

            // Update function table with endpoint configuration
            // Function table has been created above
            if let Some(f) = function_meta["function"].as_table_mut() {
                f.extend(endpoint_table)
            }
        }
        Role::Cron(params) => {
            let mut cron_table = toml_edit::Table::new();
            cron_table["schedule"] = toml_edit::value(params.schedule.to_string());

            // Update function table with cron configuration
            // Function table has been created above
            if let Some(f) = function_meta["function"].as_table_mut() {
                f.extend(cron_table)
            }
        }
    }

    // Parse json later
    let environment: String = parsed_function
        .role
        .environment()
        .iter()
        .map(|(k, v)| format!("{{\"key\": \"{k}\", \"value\": \"{v}\"}}"))
        .collect::<Vec<String>>()
        .join(",");

    if !environment.is_empty() {
        function_meta["function"]["environment"] = toml_edit::value(format!("[{environment}]"));
    }

    functions_meta.push(function_meta.into());

    Ok(())
}

/// Write dependencies required to run a lambda into Cargo.toml
fn deps(
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
            serde_json.insert("version", toml_edit::value("1.0.140"));
        }

        if let Some(reqwest) = doc["dependencies"]["reqwest"]
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut()
        {
            reqwest.insert("version", toml_edit::value("0.12.15"));
            reqwest.insert("default-features", toml_edit::value(false));
            reqwest.insert(
                "features",
                toml_edit::Array::from_iter(["rustls-tls"]).into(),
            );
        }
    }

    match parsed_function.role {
        Role::Cron(_) | Role::Worker(_) => {
            doc["dependencies"]["lambda_runtime"]
                .or_insert(toml_edit::Table::new().into())
                .as_table_mut()
                .map(|t| t.insert("version", toml_edit::value("0.13.0")));
        }
        Role::Endpoint(_) => {
            doc["dependencies"]["lambda_http"]
                .or_insert(toml_edit::Table::new().into())
                .as_table_mut()
                .map(|t| t.insert("version", toml_edit::value("0.14.0")));
        }
    };

    doc["dependencies"]["aws_lambda_events"]
        .or_insert(toml_edit::Table::new().into())
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("0.16.0")));

    doc["dependencies"]["aws-config"]
        .or_insert(toml_edit::Table::new().into())
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("1.0.1")));

    doc["dependencies"]["aws-sdk-ssm"]
        .or_insert(toml_edit::Table::new().into())
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("1.59.0")));

    doc["dependencies"]["aws-sdk-sqs"]
        .or_insert(toml_edit::Table::new().into())
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("1.62.0")));

    if let Some(tokio_dep) = doc["dependencies"]["tokio"]
        .or_insert(toml_edit::Table::new().into())
        .as_table_mut()
    {
        tokio_dep.insert("version", toml_edit::value("1.43.0"));
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
    relative_path: &str,
    rust_name: &str,
    crate_name: &str,
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
        format!("use {crate_name}::{rust_name};")
    } else {
        format!("use {crate_name}::{module_path}::{rust_name};")
    };

    Ok(import_statement)
}

/// Delete the macro attributes from a file
/// and copy it to the destination folder.
fn clean_copy(
    src_path_full: &Path,
    dst_dir: &Path,
    file_path_relative: &Path,
    checksum: &mut FileHash,
) -> eyre::Result<()> {
    let dst_path_full = dst_dir.join(file_path_relative);
    // For all non .rs files just copy it.
    if src_path_full.extension().is_some_and(|ext| ext != "rs") {
        return fs::copy(src_path_full, &dst_path_full)
            .wrap_err_with(|| format!("Failed to copy file {src_path_full:?} -> {dst_path_full:?}"))
            .map(|_| ());
    }

    // Attempt kinetics macro replacements in .rs files.
    let content = remove_kinetics_macro(
        &fs::read_to_string(src_path_full)
            .wrap_err(format!("Failed to read file {src_path_full:?}"))?,
    )?;
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
