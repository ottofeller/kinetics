mod filehash;
pub mod pipeline;
mod templates;
use crate::crat::Crate;
use eyre::{eyre, Context};
use filehash::{FileHash, CHECKSUMS_FILENAME};
use kinetics_parser::{ParsedFunction, Parser, Role};
use regex::Regex;
use std::fs::{self};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use syn::visit::Visit;
use walkdir::WalkDir;

/// Parses source code and prepares crates for deployment
/// Stores crates inside target_directory and returns list of created paths
pub fn prepare_crates(dst: PathBuf, current_crate: Crate) -> eyre::Result<Vec<PathBuf>> {
    let mut result: Vec<PathBuf> = vec![];

    // Parse functions from source code
    let mut parser = Parser::new();

    for entry in WalkDir::new(&current_crate.path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
    {
        let content = fs::read_to_string(entry.path())?;
        let syntax = syn::parse_file(&content)?;

        // Set current file relative path for further imports resolution
        // WARN It prevents to implement parallel parsing of files and requires rework in the future
        parser.set_relative_path(entry.path().strip_prefix(&current_crate.path)?.to_str());

        parser.visit_file(&syntax);
    }

    // Clone user project into the build folder.
    let project_path = dst.join(&current_crate.name);
    let crate_clone = clone(
        &current_crate.path,
        &project_path.join("clone"),
        &parser.functions,
    )?;

    let mut function_names = Vec::new();

    // For each function create a deployment crate
    for parsed_function in parser.functions {
        for is_local in [false, true] {
            let function_name = parsed_function.func_name(is_local);
            // Function name is parsed value from kinetics_macro name attribute
            // Path example: /home/some-user/.kinetics/<crate-name>/<function-name>/<rust-function-name>
            let dst = project_path.join(&function_name);
            if !matches!(fs::exists(&dst), Ok(true)) {
                fs::create_dir_all(&dst).wrap_err("Failed to provision temp dir")?;
            }

            create_lambda_crate(
                &current_crate.path,
                &dst,
                &parsed_function,
                &current_crate.name,
                &crate_clone,
                is_local,
            )?;

            result.push(dst);
            function_names.push(function_name);
        }
    }

    workspace(&project_path, &function_names)?;

    Ok(result)
}

/// Clone the crate dir to a new directory
fn clone(src: &Path, dst: &Path, functions: &[ParsedFunction]) -> eyre::Result<Crate> {
    fs::create_dir_all(dst).wrap_err("Failed to create dir to clone the crate to")?;
    // Checksums of source files for preventing rewrite existing files
    let mut checksum = FileHash::new(dst.to_path_buf());

    // Skip source target from copying
    let src_target = src.join("target");
    // Handle Cargo.toml as a special case
    let relative_cargo_path = Path::new("Cargo.toml");
    let src_cargo_path = src.join("Cargo.toml");

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
        clean_copy(src_path, dst, src_relative, &mut checksum)?;
    }

    create_lib(src, dst, functions, &mut checksum)?;

    checksum.save().wrap_err("Failed to save checksums")?;
    clear_dir(dst, &checksum)?;

    Crate::new(dst.to_path_buf())
}

/// Create a manifest for the entire project build folder
/// with all the crates added as workspace members.
fn workspace(dst: &Path, members: &[String]) -> eyre::Result<()> {
    let dst_cargo_path = dst.join("Cargo.toml");

    // Copy Cargo.toml with modifications
    let mut cargo_toml = toml_edit::DocumentMut::new();
    let mut workspace_table = toml_edit::Table::default();
    workspace_table["resolver"] = "2".into();
    workspace_table["members"] = toml_edit::Array::from_iter(members.iter()).into();
    cargo_toml["workspace"] = workspace_table.into();

    fs::write(&dst_cargo_path, &cargo_toml.to_string())
        .wrap_err("Failed to write workspace Cargo.toml")?;

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
    // Leave an existing lib.rs as is.
    if src_lib_rs_path.exists() {
        return Ok(());
    }

    let module_definitions = functions
        .iter()
        .filter_map(|f| {
            // Take the first path component from each module in the src folder, and export it.
            match Path::new(&f.relative_path)
                .strip_prefix("src")
                .ok()?
                .with_extension("")
                .components()
                .next()
            {
                Some(Component::Normal(comp)) => {
                    comp.to_str().map(|module| format!("pub mod {module};\n"))
                }
                _ => None,
            }
        })
        .collect::<String>();

    if checksum.update(
        relative_lib_path.to_path_buf(),
        &FileHash::hash_from_bytes(&module_definitions)
            .wrap_err("Failed to calculate hash from bytes of src/lib.rs")?,
    ) {
        fs::write(dst.join(&relative_lib_path), module_definitions)
            .wrap_err("Failed to write src/lib.rs")?;
    }

    Ok(())
}

/// Create crate with the code necessary to build lambda
///
/// Set up the main() function according to cargo lambda guides,
/// and add the lambda code right to main.rs
fn create_lambda_crate(
    src: &Path,
    dst: &Path,
    parsed_function: &ParsedFunction,
    project_name: &str,
    crate_clone: &Crate,
    is_local: bool,
) -> eyre::Result<()> {
    let mut checksum = FileHash::new(dst.to_path_buf());

    let src_dir_path = dst.join("src");
    if !matches!(fs::exists(&src_dir_path), Ok(true)) {
        fs::create_dir(&src_dir_path).wrap_err("Failed to create src folder")?;
    }

    let main_rs_path = src_dir_path.join("main.rs");

    let fn_import = import_statement(
        &parsed_function.relative_path,
        &parsed_function.rust_function_name,
        &crate_clone.name,
    )?;

    let rust_function_name = parsed_function.rust_function_name.clone();
    let main_code = match &parsed_function.role {
        Role::Endpoint(_) => templates::endpoint(&fn_import, &rust_function_name, is_local),
        Role::Worker(_) => templates::worker(&fn_import, &rust_function_name, is_local),
        Role::Cron(_) => templates::cron(&fn_import, &rust_function_name, is_local),
    };

    let item: syn::File = syn::parse_str(&main_code)?;
    let main_rs_content = prettyplease::unparse(&item);
    if checksum.update(
        main_rs_path.strip_prefix(dst)?.to_path_buf(),
        &FileHash::hash_from_bytes(&main_rs_content)
            .wrap_err("Failed to calculate hash for bytes of main.rs")?,
    ) {
        fs::write(&main_rs_path, &main_rs_content).wrap_err("Failed to write main.rs")?;
    }

    let mut doc: toml_edit::DocumentMut = fs::read_to_string(src.join("Cargo.toml"))?.parse()?;

    doc["package"]["name"] = format!(
        "{}-{}-kinetics-build",
        project_name,
        parsed_function.func_name(is_local)
    )
    .into();
    let mut aot = toml_edit::ArrayOfTables::new();
    let mut new_bin = toml_edit::Table::new();
    new_bin["name"] = toml_edit::value("bootstrap");
    new_bin["path"] = toml_edit::value("src/main.rs");
    aot.push(new_bin);
    doc["bin"] = toml_edit::Item::ArrayOfTables(aot);

    if let Some(kinetics_meta) = doc["package"]["metadata"]["kinetics"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
    {
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
        kinetics_meta.insert("function", toml_edit::Item::Table(function_table));

        match &parsed_function.role {
            Role::Worker(params) => {
                let mut queue_table = toml_edit::Table::new();
                queue_table["name"] = toml_edit::value(&name);
                queue_table["alias"] = toml_edit::value(params.queue_alias.clone().unwrap());
                queue_table["concurrency"] = toml_edit::value(params.concurrency as i64);
                queue_table["fifo"] = toml_edit::value(params.fifo);

                let mut named_table = toml_edit::Table::new();
                named_table.set_implicit(true); // Don't create an empty queue table
                named_table.insert(&name, toml_edit::Item::Table(queue_table));

                kinetics_meta.insert("queue", toml_edit::Item::Table(named_table));
            }
            Role::Endpoint(params) => {
                let mut endpoint_table = toml_edit::Table::new();
                endpoint_table["url_path"] = toml_edit::value(&params.url_path);

                endpoint_table["queues"] = toml_edit::value(
                    serde_json::to_string(&params.queues.clone().unwrap_or(vec![])).unwrap(),
                );

                // Update function table with endpoint configuration
                // Function table has been created above
                if let Some(f) = kinetics_meta["function"].as_table_mut() {
                    f.extend(endpoint_table)
                }
            }
            Role::Cron(params) => {
                let mut cron_table = toml_edit::Table::new();
                cron_table["schedule"] = toml_edit::value(params.schedule.to_string());

                // Update function table with cron configuration
                // Function table has been created above
                if let Some(f) = kinetics_meta["function"].as_table_mut() {
                    f.extend(cron_table)
                }
            }
        }

        let environment = parsed_function.role.environment().iter();

        if let Some(e) = kinetics_meta["environment"]
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut()
        {
            e.extend(environment)
        }
    }

    if matches!(parsed_function.role, Role::Cron(_) | Role::Worker(_))
        || (matches!(parsed_function.role, Role::Endpoint(_)) && is_local)
    {
        doc["dependencies"]["serde_json"]
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut()
            .map(|t| t.insert("version", toml_edit::value("1.0.140")));
    }

    doc["dependencies"]["aws_lambda_events"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("0.16.0")));

    doc["dependencies"]["aws-config"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("1.0.1")));

    doc["dependencies"]["aws-sdk-ssm"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("1.59.0")));

    doc["dependencies"]["aws-sdk-sqs"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("1.62.0")));

    if let Some(tokio_dep) = doc["dependencies"]["tokio"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
    {
        tokio_dep.insert("version", toml_edit::value("1.43.0"));

        tokio_dep.insert(
            "features",
            toml_edit::value(toml_edit::Array::from_iter(vec!["full"])),
        );
    }

    if let Some(deps_table) = doc["dependencies"].as_table_mut() {
        deps_table.remove("kinetics-macro");
    }

    let target_cargo_path = dst.join("Cargo.toml");
    let crate_path = relative_path(dst, &crate_clone.path)?;
    doc["dependencies"][crate_clone.name.clone()]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .map(|t| t.insert("path", toml_edit::value(crate_path.to_str().unwrap())));

    let doc_string = doc.to_string();
    if checksum.update(
        target_cargo_path.strip_prefix(dst)?.to_path_buf(),
        &FileHash::hash_from_bytes(&doc_string)
            .wrap_err("Failed to calculate hash from bytes of Cargo.toml")?,
    ) {
        fs::write(&target_cargo_path, &doc_string).wrap_err("Failed to write Cargo.toml")?;
    }

    checksum.save().wrap_err("Failed to save checksums")?;
    clear_dir(dst, &checksum)?;

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

    let mut module_path_parts = Vec::new();

    for component in relative_path.components() {
        if let std::path::Component::Normal(os_str) = component {
            let s = os_str.to_str().unwrap();
            module_path_parts.push(s);
        }
    }

    // Handle lib.rs, main.rs (root module)
    let is_root_module =
        relative_path == Path::new("lib.rs") || relative_path == Path::new("main.rs");

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

    // If module path is empty then the function is locate in the main.rs file
    let import_statement = if module_path.is_empty() {
        "".to_string()
    } else {
        format!("use {crate_name}::{}::{};", module_path, rust_name)
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
    let re_endpoint = Regex::new(r"(?m)^\s*#\s*\[\s*endpoint[^]]*]\s*$")?;
    let re_cron = Regex::new(r"(?m)^\s*#\s*\[\s*cron[^]]*]\s*$")?;
    let re_worker = Regex::new(r"(?m)^\s*#\s*\[\s*worker[^]]*]\s*$")?;

    let re_import = Regex::new(
        r"(?m)^\s*use\s+kinetics_macro(\s*::\s*(\w+|\{\s*\w+(\s*,\s*\w+)*\s*}))?\s*;\s*$",
    )?;

    let mut content = fs::read_to_string(src_path_full)
        .wrap_err(format!("Failed to read file {src_path_full:?}"))?;

    content = re_endpoint.replace_all(&content, "").to_string();
    content = re_worker.replace_all(&content, "").to_string();
    content = re_cron.replace_all(&content, "").to_string();
    let new_content = re_import.replace_all(&content, "").into_owned();
    if checksum.update(
        file_path_relative.to_path_buf(),
        &FileHash::hash_from_bytes(&new_content).wrap_err_with(|| {
            format!("Failed to calculate hash from bytes of {src_path_full:?}")
        })?,
    ) {
        fs::write(&dst_path_full, &new_content)
            .wrap_err_with(|| format!("Failed to write {dst_path_full:?}"))?;
    }

    Ok(())
}

fn relative_path(src: &Path, dst: &Path) -> eyre::Result<PathBuf> {
    let mut parent = src;
    while let Some(new_parent) = parent.parent() {
        parent = new_parent;
        if let Ok(stripped_dst) = dst.strip_prefix(parent) {
            // Found mutual parent
            return Ok(src
                .strip_prefix(parent)?
                .iter()
                .map(|_c| Component::ParentDir.as_os_str())
                .chain(stripped_dst.iter())
                .collect());
        }
    }

    Err(eyre!(
        "No common ancestor found of paths {src:?} and {dst:?}"
    ))
}
