use crate::crat::Crate;
use crate::function::Function;
use crate::parser::{ParsedFunction, Role};
use eyre::Context;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use syn::visit::Visit;
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
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let content = fs::read_to_string(entry.path())?;
        let syntax = syn::parse_file(&content)?;

        // Set current file relative path for further imports resolution
        // WARN It prevents to implement parallel parsing of files and requires rework in the future
        parser.set_relative_path(entry.path().strip_prefix(&current_crate.path)?.to_str());

        parser.visit_file(&syntax);
    }

    fs::create_dir_all(&target_directory).wrap_err("Failed to create directory")?;

    for parsed_function in parser.functions {
        // Function name is parsed value from skymacro name attribute
        // Path example: /home/some-user/.sky/<crate-name>/<function-name>/<rust-function-name>
        let dst = target_directory
            .join(&current_crate.name)
            .join(parsed_function.role.rust_function_name());

        clone(&current_crate.path, &dst)?;
        cleanup(&dst)?;
        inject(&dst, &parsed_function)?;

        result.push(dst);
    }

    Ok(result)
}

/// Build all assets and CFN templates
pub fn build(functions: &Vec<Function>) -> eyre::Result<()> {
    let threads: Vec<_> = functions
        .into_iter()
        .cloned()
        .map(|function| std::thread::spawn(move || function.build()))
        .collect();

    for thread in threads {
        thread.join().unwrap()?;
    }

    println!("Done!");
    eyre::Ok(())
}

/// Clone the crate dir to a new directory
fn clone(src: &Path, dst: &Path) -> eyre::Result<()> {
    if dst.exists() {
        // Cleanup existing contents
        let src_path = dst.join("src");
        let cargo_path = dst.join("Cargo.toml");

        if src_path.exists() {
            match fs::remove_dir_all(&src_path) {
                Ok(_) => {}
                Err(e) => {
                    println!("Failed to delete src dir: {:?}, {:?}", &src_path, e);
                }
            }
        }

        if cargo_path.exists() {
            match fs::remove_file(&cargo_path) {
                Ok(_) => {}
                Err(e) => {
                    println!("Failed to delete Cargo.toml: {:?}, {:?}", &cargo_path, e);
                }
            }
        }
    }

    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let src_path = entry.path();

        if src_path.eq(&src.join("target")) {
            continue;
        }

        // Strip leading path from source to create relative path in destination
        let src_relative = entry
            .path()
            .strip_prefix(src)
            .unwrap_or_else(|_| entry.path());

        let dst_path = dst.join(src_relative);

        if src_path.is_dir() {
            fs::create_dir_all(&dst_path).wrap_err("Create dir failed")?;
        } else {
            fs::copy(&src_path, &dst_path).wrap_err("Copying file failed")?;
        }
    }

    Ok(())
}

/// Inject the code which is necessary to build lambda
///
/// Set up the main() function according to cargo lambda guides, and add the lambda code right to main.rs
fn inject(dst: &PathBuf, function_info: &ParsedFunction) -> eyre::Result<()> {
    let main_rs_path = dst.join("src").join("main.rs");
    let lib_rs_path = dst.join("src").join("lib.rs");

    // Move lib.rs to main.rs
    if lib_rs_path.exists() {
        let lib_content = fs::read_to_string(&lib_rs_path).wrap_err("Failed to read lib.rs")?;

        fs::write(&main_rs_path, format!("{}\nfn main() {{}}", lib_content))
            .wrap_err("Failed to write main.rs")?;

        fs::remove_file(&lib_rs_path).wrap_err("Failed to delete lib.rs")?;
    }

    if !main_rs_path.exists() {
        fs::write(&main_rs_path, "fn main() {}").wrap_err("Failed to create main.rs")?;
    }

    let source_code = fs::read_to_string(&main_rs_path).wrap_err("Reading main.rs failed")?;

    let re = Regex::new(r"fn\s+main\s*\(.*?\)\s*\{[^}]*}").wrap_err("Failed to prepare regex")?;

    let import_statement = import_statement(
        function_info.relative_path.as_str(),
        function_info.rust_function_name.as_str(),
    )?;

    let rust_function_name = function_info.rust_function_name.clone();

    let new_main_code = match &function_info.role {
        Role::Endpoint(_) => {
            format!(
                "{import_statement}
            use lambda_http::{{run, service_fn}};\n\
            #[tokio::main]\n\
            async fn main() -> Result<(), lambda_http::Error> {{\n\
                let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                println!(\"Provisioning secrets\");
                let secrets_client = aws_sdk_ssm::Client::new(&config);
                let secrets_names_env = \"SECRETS_NAMES\";
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
                use aws_lambda_events::{{lambda_function_urls::LambdaFunctionUrlRequest, sqs::SqsEvent, sqs::SqsBatchResponse}};\n\n\
                #[tokio::main]\n\
                async fn main() -> Result<(), Error> {{\n\
                    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                    println!(\"Provisioning secrets\");
                    let secrets_client = aws_sdk_ssm::Client::new(&config);
                    let secrets_names_env = \"SECRETS_NAMES\";
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
    };

    let item: syn::File = syn::parse_str(&new_main_code)?;

    fs::write(
        &main_rs_path,
        re.replace(&source_code, prettyplease::unparse(&item))
            .as_ref(),
    )
    .wrap_err(format!("Failed to write: {main_rs_path:?}"))?;

    let cargo_toml_path = dst.join("Cargo.toml");
    let mut doc: toml_edit::DocumentMut = fs::read_to_string(&cargo_toml_path)?.parse()?;

    if !doc.contains_array_of_tables("bin") {
        let mut aot = toml_edit::ArrayOfTables::new();
        let mut new_bin = toml_edit::Table::new();
        new_bin["name"] = toml_edit::value("bootstrap");
        new_bin["path"] = toml_edit::value("src/main.rs");
        aot.push(new_bin);
        doc["bin"] = toml_edit::Item::ArrayOfTables(aot);
    }

    doc["package"]["metadata"]["sky"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .map(|sky_meta| {
            if !sky_meta.contains_key("function") {
                let (url_path, name, role) = match &function_info.role {
                    Role::Endpoint(p) => (p.url_path.as_str(), &p.name, "endpoint"),
                    Role::Worker(p) => ("", &p.name, "worker"),
                };

                let mut function_table = toml_edit::Table::new();
                function_table["name"] = toml_edit::value(name);
                function_table["role"] = toml_edit::value(role);
                if !url_path.is_empty() {
                    function_table["url_path"] = toml_edit::value(url_path);
                }

                sky_meta.insert("function", toml_edit::Item::Table(function_table));
            };

            let environment = function_info.role.environment().iter();

            sky_meta["environment"]
                .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                .as_table_mut()
                .map(|e| e.extend(environment));
        });

    doc["dependencies"]["aws-config"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("1.0.1")));

    doc["dependencies"]["aws-sdk-ssm"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .map(|t| t.insert("version", toml_edit::value("1.59.0")));

    doc["dependencies"]["tokio"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .map(|t| {
            t.insert("version", toml_edit::value("1.43.0"));
            let features = toml_edit::Array::from_iter(vec!["full"]);
            t.insert("features", toml_edit::value(features));
        });

    fs::write(&cargo_toml_path, doc.to_string())?;
    Ok(())
}

/// Generate the import statement for the function which is being deployed
/// as a lambda
fn import_statement(relative_path: &str, rust_name: &str) -> eyre::Result<String> {
    let relative_path = PathBuf::from_str(
        relative_path
            .strip_prefix("src/")
            .unwrap_or_else(|| relative_path),
    )?;

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
    // Delete the macro attributes from everywhere in the crate
    for entry in WalkDir::new(dst)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let path = entry.path();

        if path.is_dir() {
            continue;
        }

        let mut content = fs::read_to_string(&path)
            .wrap_err(format!("Failed to read: {path:?}"))
            .wrap_err("Failed to read file")?;

        let re_endpoint = Regex::new(r"(?m)^\s*#\s*\[\s*endpoint[^]]*]\s*$")?;
        let re_worker = Regex::new(r"(?m)^\s*#\s*\[\s*worker[^]]*]\s*$")?;

        let re_import = Regex::new(
            r"(?m)^\s*use\s+skymacro(\s*::\s*(\w+|\{\s*\w+(\s*,\s*\w+)*\s*}))?\s*;\s*$",
        )?;

        content = re_endpoint.replace_all(&content, "").to_string();
        content = re_worker.replace_all(&content, "").to_string();
        let new_content = re_import.replace_all(&content, "");

        fs::write(&path, new_content.as_ref()).wrap_err(format!("Failed to write: {path:?}"))?;
    }

    let cargo_toml_path = dst.join("Cargo.toml");
    let mut doc: toml_edit::DocumentMut = fs::read_to_string(&cargo_toml_path)?.parse()?;

    if let Some(deps_table) = doc["dependencies"].as_table_mut() {
        deps_table.remove("skymacro");
    }

    fs::write(&cargo_toml_path, doc.to_string())?;
    Ok(())
}
