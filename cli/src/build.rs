use crate::crat::Crate;
use crate::parser::{ParsedFunction, Role};
use eyre::Context;
use regex::Regex;
use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use syn::visit::Visit;
use twox_hash::XxHash64;
use walkdir::WalkDir;

/// Parses source code and prepares crates for deployment
/// Stores crates inside target_directory and returns list of created paths
pub fn prepare_crates(
    target_directory: PathBuf,
    current_crate: Crate,
) -> eyre::Result<Vec<PathBuf>> {
    let mut result: Vec<PathBuf> = vec![];

    // Parse functions from source code
    let mut parser = crate::parser::Parser::new();

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

    for parsed_function in parser.functions {
        // Function name is parsed value from kinetics_macro name attribute
        // Path example: /home/some-user/.kinetics/<crate-name>/<function-name>/<rust-function-name>
        let dst = target_directory
            .join(&current_crate.name)
            .join(func_name(&parsed_function));

        let src = &current_crate.path;
        clone(src, &dst)?;
        cleanup(&dst)?;
        inject(src, &dst, &parsed_function)?;

        result.push(dst);
    }

    Ok(result)
}

/// Clone the crate dir to a new directory
fn clone(src: &Path, dst: &Path) -> eyre::Result<()> {
    // Checksums of source files for preventing rewrite existing files
    let mut checksum = FileHash::new(dst.to_path_buf());

    // Skip source target from copying
    let src_target = src.join("target");

    for entry in WalkDir::new(src)
        .into_iter()
        .filter_map(|e| e.ok())
        // Skip the target dir, cargo lambda use it (if exist) for incremental builds
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

        let new_hash = FileHash::hash_from_path(src_path).wrap_err("Failed to calculate hash")?;

        // If src file has been modified, copy it to the destination
        let old_hash = checksum
            .inner
            // insert() returns the old value if it exists and updates it
            .insert(src_relative.to_path_buf(), new_hash.clone())
            .unwrap_or_default();

        if new_hash != old_hash {
            fs::copy(src_path, &dst_path).wrap_err("Copying file failed")?;
        }
    }

    checksum.save().wrap_err("Failed to save checksums")?;

    // TODO Remove files that are not present in the source directory
    // but still exist in the target directory
    Ok(())
}

/// Inject the code which is necessary to build lambda
///
/// Set up the main() function according to cargo lambda guides, and add the lambda code right to main.rs
fn inject(src: &Path, dst: &Path, parsed_function: &ParsedFunction) -> eyre::Result<()> {
    let tmp_dir = &dst.join(".temp");
    fs::create_dir_all(tmp_dir).wrap_err("Failed to create .temp directory")?;

    // Work with the tmp file to avoid modifying the original file without real changes.
    let tmp_main_rs_path = tmp_dir.join("main.rs");
    let main_rs_path = dst.join("src").join("main.rs");
    let lib_rs_path = dst.join("src").join("lib.rs");

    // Copy existing main.rs to a temporary file
    let _ = fs::copy(&main_rs_path, &tmp_main_rs_path);

    // Move lib.rs to main.rs
    if lib_rs_path.exists() {
        let lib_content = fs::read_to_string(&lib_rs_path).wrap_err("Failed to read lib.rs")?;

        fs::write(
            &tmp_main_rs_path,
            format!("{}\nfn main() {{}}", lib_content),
        )
        .wrap_err("Failed to write main.rs")?;

        fs::remove_file(&lib_rs_path).wrap_err("Failed to delete lib.rs")?;
    }

    if !tmp_main_rs_path.exists() {
        fs::write(&tmp_main_rs_path, "fn main() {}").wrap_err("Failed to create main.rs")?;
    }

    let source_code = fs::read_to_string(&tmp_main_rs_path).wrap_err("Reading main.rs failed")?;
    let re = Regex::new(r"fn\s+main\s*\(.*?\)\s*\{[^}]*}").wrap_err("Failed to prepare regex")?;

    let import_statement = import_statement(
        parsed_function.relative_path.as_str(),
        parsed_function.rust_function_name.as_str(),
    )?;

    let rust_function_name = parsed_function.rust_function_name.clone();

    let new_main_code = match &parsed_function.role {
        Role::Endpoint(_) => {
            format!(
                "{import_statement}
            use lambda_http::{{run, service_fn}};\n\
            #[tokio::main]\n\
            async fn main() -> Result<(), lambda_http::Error> {{\n\
                let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                println!(\"Fetching secrets\");
                let secrets_client = aws_sdk_ssm::Client::new(&config);
                let secrets_names_env = \"KINETICS_SECRETS_NAMES\";
                let mut secrets = std::collections::HashMap::new();

                for secret_name in std::env::var(secrets_names_env)?
                    .split(\",\")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {{
                    let desc = secrets_client
                        .get_parameter()
                        .name(secret_name.clone())
                        .with_decryption(true)
                        .send()
                        .await?;

                    let result = desc.parameter.unwrap();

                    let tags = secrets_client
                        .list_tags_for_resource()
                        .resource_type(aws_sdk_ssm::types::ResourceTypeForTagging::Parameter)
                        .resource_id(secret_name.clone())
                        .send()
                        .await?
                        .tag_list
                        .unwrap_or_default();

                    let name = match tags.iter().find(|t| t.key() == \"original_name\") {{
                        Some(tag) => tag.value(),
                        None => &secret_name.clone(),
                    }};

                    let secret_value = result.value().unwrap();
                    secrets.insert(name.into(), secret_value.to_string());
                }}

                println!(\"Serving requests\");

                run(service_fn(|event| async {{
                    match {rust_function_name}(event, &secrets).await {{
                        Ok(response) => Ok(response),
                        Err(err) => {{
                            eprintln!(\"Error occurred while handling request: {{:?}}\", err);
                            Err(err)
                        }}
                    }}
                }})).await
            }}\n\n"
            )
        }
        Role::Worker(_) => {
            format!(
                "{import_statement}
                use lambda_runtime::{{LambdaEvent, Error, run, service_fn}};\n\
                use aws_lambda_events::{{sqs::SqsEvent, sqs::SqsBatchResponse}};\n\n\
                #[tokio::main]\n\
                async fn main() -> Result<(), Error> {{\n\
                    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                    println!(\"Provisioning secrets\");
                    let secrets_client = aws_sdk_ssm::Client::new(&config);
                    let secrets_names_env = \"KINETICS_SECRETS_NAMES\";
                    let mut secrets = std::collections::HashMap::new();

                    for secret_name in std::env::var(secrets_names_env)?
                        .split(\",\")
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                    {{
                        let desc = secrets_client
                            .get_parameter()
                            .name(secret_name.clone())
                            .with_decryption(true)
                            .send()
                            .await?;

                        let result = desc.parameter.unwrap();

                        let tags = secrets_client
                            .list_tags_for_resource()
                            .resource_type(aws_sdk_ssm::types::ResourceTypeForTagging::Parameter)
                            .resource_id(secret_name.clone())
                            .send()
                            .await?
                            .tag_list
                            .unwrap_or_default();

                        let name = match tags.iter().find(|t| t.key() == \"original_name\") {{
                            Some(tag) => tag.value(),
                            None => &secret_name.clone(),
                        }};

                        let secret_value = result.value().unwrap();
                        secrets.insert(name.into(), secret_value.to_string());
                    }}

                    println!(\"Serving requests\");

                    run(service_fn(|event| async {{
                        match {rust_function_name}(event, &secrets).await {{
                            Ok(response) => Ok(response),
                            Err(err) => {{
                                eprintln!(\"Error occurred while handling request: {{:?}}\", err);
                                Err(err)
                            }}
                        }}
                    }})).await
                }}\n\n"
            )
        }
        Role::Cron(_) => {
            format!(
                "{import_statement}
                use lambda_runtime::{{LambdaEvent, Error, run, service_fn}};\n\
                use aws_lambda_events::eventbridge::EventBridgeEvent;\n\
                #[tokio::main]\n\
                async fn main() -> Result<(), Error> {{\n\
                    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                    println!(\"Provisioning secrets\");
                    let secrets_client = aws_sdk_ssm::Client::new(&config);
                    let secrets_names_env = \"KINETICS_SECRETS_NAMES\";
                    let mut secrets = std::collections::HashMap::new();

                    for secret_name in std::env::var(secrets_names_env)?
                        .split(\",\")
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                    {{
                        let desc = secrets_client
                            .get_parameter()
                            .name(secret_name.clone())
                            .with_decryption(true)
                            .send()
                            .await?;

                        let result = desc.parameter.unwrap();

                        let tags = secrets_client
                            .list_tags_for_resource()
                            .resource_type(aws_sdk_ssm::types::ResourceTypeForTagging::Parameter)
                            .resource_id(secret_name.clone())
                            .send()
                            .await?
                            .tag_list
                            .unwrap_or_default();

                        let name = match tags.iter().find(|t| t.key() == \"original_name\") {{
                            Some(tag) => tag.value(),
                            None => &secret_name.clone(),
                        }};

                        let secret_value = result.value().unwrap();
                        secrets.insert(name.into(), secret_value.to_string());
                    }}

                    println!(\"Serving requests\");

                    run(service_fn(|_event: LambdaEvent<EventBridgeEvent<serde_json::Value>>| async {{
                        match cron(&secrets).await {{
                            Ok(()) => Ok(()),
                            Err(err) => {{
                                eprintln!(\"Error occurred while handling request: {{:?}}\", err);
                                Err(err)
                            }}
                        }}
                    }}))
                    .await
                }}\n\n"
            )
        }
    };

    let item: syn::File = syn::parse_str(&new_main_code)?;

    write_if_changed(
        &main_rs_path,
        re.replace(&source_code, prettyplease::unparse(&item))
            .to_string(),
    )?;

    let mut doc: toml_edit::DocumentMut = fs::read_to_string(src.join("Cargo.toml"))?.parse()?;

    if !doc.contains_array_of_tables("bin") {
        let mut aot = toml_edit::ArrayOfTables::new();
        let mut new_bin = toml_edit::Table::new();
        new_bin["name"] = toml_edit::value("bootstrap");
        new_bin["path"] = toml_edit::value("src/main.rs");
        aot.push(new_bin);
        doc["bin"] = toml_edit::Item::ArrayOfTables(aot);
    }

    if let Some(kinetics_meta) = doc["package"]["metadata"]["kinetics"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
    {
        let role_str = match &parsed_function.role {
            Role::Endpoint(_) => "endpoint",
            Role::Cron(_) => "cron",
            Role::Worker(_) => "worker",
        };

        let name = func_name(parsed_function);

        // Create a function table for both roles
        let mut function_table = toml_edit::Table::new();
        function_table["name"] = toml_edit::value(&name);
        function_table["role"] = toml_edit::value(role_str);
        kinetics_meta.insert("function", toml_edit::Item::Table(function_table));

        match &parsed_function.role {
            Role::Worker(params) => {
                let mut queue_table = toml_edit::Table::new();
                queue_table["name"] = toml_edit::value(&name);
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

                // Update function table with endpoint configuration
                // Function table has been created above
                if let Some(f) = kinetics_meta["function"].as_table_mut() {
                    f.extend(endpoint_table)
                }
            }
            Role::Cron(params) => {
                let mut cron_table = toml_edit::Table::new();
                cron_table["schedule"] = toml_edit::value(&params.schedule.to_string());

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

    doc["dependencies"]["aws-config"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("1.0.1")));

    doc["dependencies"]["aws-sdk-ssm"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("1.59.0")));

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

    write_if_changed(dst.join("Cargo.toml"), doc.to_string())
        .wrap_err("Failed to replace Cargo.toml")?;

    let _ = fs::remove_dir_all(tmp_dir).wrap_err("Failed to remove temporary directory");
    Ok(())
}

/// Generate the import statement for the function which is being deployed
/// as a lambda
fn import_statement(relative_path: &str, rust_name: &str) -> eyre::Result<String> {
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
        format!("use {}::{};", module_path, rust_name)
    };

    Ok(import_statement)
}

/// Clean up scaffolding required for deploying a function
fn cleanup(dst: &Path) -> eyre::Result<()> {
    let re_endpoint = Regex::new(r"(?m)^\s*#\s*\[\s*endpoint[^]]*]\s*$")?;
    let re_cron = Regex::new(r"(?m)^\s*#\s*\[\s*cron[^]]*]\s*$")?;
    let re_worker = Regex::new(r"(?m)^\s*#\s*\[\s*worker[^]]*]\s*$")?;

    let re_import = Regex::new(
        r"(?m)^\s*use\s+kinetics_macro(\s*::\s*(\w+|\{\s*\w+(\s*,\s*\w+)*\s*}))?\s*;\s*$",
    )?;

    // Delete the macro attributes from everywhere in the crate
    for entry in WalkDir::new(dst.join("src"))
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
    {
        let path = entry.path();

        if path.is_dir() {
            continue;
        }

        let mut content = fs::read_to_string(path)
            .wrap_err(format!("Failed to read: {path:?}"))
            .wrap_err("Failed to read file")?;

        content = re_endpoint.replace_all(&content, "").to_string();
        content = re_worker.replace_all(&content, "").to_string();
        content = re_cron.replace_all(&content, "").to_string();
        let new_content = re_import.replace_all(&content, "");
        write_if_changed(path, new_content.as_ref())?;
    }

    Ok(())
}

/// Generate lambda function name out of Rust function name or macro attribute
///
/// By default use the Rust function plus crate path as the function name. Convert
/// some-name to SomeName, and do other transformations in order to comply with Lambda
/// function name requirements.
pub fn func_name(parsed_function: &ParsedFunction) -> String {
    let rust_name = &parsed_function.rust_function_name;

    let default_func_name = format!(
        "{}{rust_name}",
        parsed_function
            .relative_path
            .as_str()
            .split(&['-', '.', '/'])
            .map(|s| match s.chars().next() {
                Some(first) => first.to_uppercase().collect::<String>() + &s[1..],
                None => String::new(),
            })
            .collect::<String>()
    );

    // TODO Check the name for uniqueness
    parsed_function
        .role
        .name()
        .unwrap_or(&default_func_name)
        .to_string()
}

/// Stores files hashes on the disk to avoid rebuilding on unchanged files
/// cargo lambda rebuilds crate if file timestamp changed
struct FileHash {
    path: PathBuf,
    inner: HashMap<PathBuf, String>,
}

impl FileHash {
    fn new(dst: PathBuf) -> Self {
        let path = dst.join(".checksums");

        // Relative path -> hash of the file
        let checksums: HashMap<PathBuf, String> = {
            match fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => HashMap::new(),
            }
        };

        FileHash {
            inner: checksums,
            path,
        }
    }

    fn save(&self) -> eyre::Result<()> {
        Ok(fs::write(
            &self.path,
            serde_json::to_string_pretty(&self.inner)?,
        )?)
    }

    fn hash_from_path<P: AsRef<Path>>(path: P) -> eyre::Result<String> {
        let mut file = File::open(path).wrap_err("Failed to open file")?;
        let mut hasher = XxHash64::default();
        let mut buffer = [0; 8192];

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.write(&buffer[..bytes_read]);
        }

        Ok(format!("{:x}", hasher.finish()))
    }

    fn hash_from_bytes<C: AsRef<[u8]>>(contents: C) -> eyre::Result<String> {
        let mut hasher = XxHash64::default();
        hasher.write(contents.as_ref());
        Ok(format!("{:x}", hasher.finish()))
    }
}

/// Check the checksum of a file before write to ensure it hasn't changed
fn write_if_changed<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> eyre::Result<()> {
    let path_hash = FileHash::hash_from_path(&path).unwrap_or("path".to_string());
    let contents_hash = FileHash::hash_from_bytes(&contents).unwrap_or("content".to_string());

    if path_hash == contents_hash {
        Ok(())
    } else {
        fs::write(&path, &contents)
            .wrap_err(format!("Failed to write: {}", path.as_ref().display()))
    }
}
