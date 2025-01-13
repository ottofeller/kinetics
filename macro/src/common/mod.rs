pub mod attrs;
use crate::FunctionRole;
use attrs::{Attrs, Environment};
use eyre::{eyre, ContextCompat, WrapErr};
use proc_macro::SourceFile;
use proc_macro::TokenStream;
use regex::Regex;
use std::fs::read_to_string;
use std::fs::write;
use std::path::Path;
use std::path::PathBuf;
use syn::{parse_macro_input, ItemFn};

/// Where all the projects are copied
pub fn skypath() -> eyre::Result<PathBuf> {
    Ok(Path::new(&std::env::var("HOME").wrap_err("Can not read HOME env var")?).join(".sky"))
}

/// Clone the crate dir to a new directory
fn clone(src: &Path, dst: &Path) {
    if dst.exists() {
        // For some reason am existing directory is not being deleted
        // when the macro is run in IDE.
        match std::fs::remove_dir_all(&dst) {
            Ok(_) => {}
            Err(e) => {
                println!("Failed to delete old dir: {:?}, {:?}", &dst, e);
            }
        }
    }

    fn clone_recursively(src: &Path, dst: &Path) -> eyre::Result<()> {
        let crate_root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        std::fs::create_dir_all(dst).wrap_err("Create dir failed")?;
        for entry in std::fs::read_dir(src).wrap_err("Read dir failed")? {
            let entry = entry.wrap_err("No entry")?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.eq(&Path::new(&crate_root).join("target")) {
                continue;
            }

            if src_path.is_dir() {
                clone_recursively(&src_path, &dst_path).wrap_err("Cloning recursively failed")?;
            } else {
                std::fs::copy(&src_path, &dst_path).wrap_err("Copying file failed")?;
            }
        }
        Ok(())
    }

    clone_recursively(&src, &dst).unwrap();
}

/// Generate lambda function name out of Rust function name or macro attribute
///
/// By default use the Rust function plus crate path as the function name. Convert
/// some-name to SomeName, and do other transformations in order to comply with Lambda
/// function name requirements.
pub fn func_name(attrs: &Attrs, file: &SourceFile, rust_name: &str) -> eyre::Result<String> {
    let default_func_name = format!(
        "{}{rust_name}",
        &file
            .path()
            .to_str()
            .ok_or(eyre!("Failed to get file path"))?
            .split(&['-', '.', '/'])
            .map(|s| match s.chars().next() {
                Some(first) => first.to_uppercase().collect::<String>() + s.chars().as_str(),
                None => String::new(),
            })
            .collect::<String>()
    );

    // TODO Check the name for uniqueness
    Ok(attrs
        .name()
        .or(Some(default_func_name))
        .unwrap()
        .to_string())
}

/// Inject the code which is necessary to build lambda
///
/// Set up the main() function according to cargo lambda guides, and add the lambda code right to main.rs
fn inject(
    dst: &Path,
    function_name: &str,
    rust_function_name: &str,
    function_code: &str,
    function_role: FunctionRole,
    environment: &Environment,
) {
    let main_rs_path = dst.join("src").join("main.rs");

    let source_code = read_to_string(&main_rs_path)
        .wrap_err("Reading main.rs failed")
        .unwrap();

    let re = Regex::new(r"fn\s+main\s*\(.*?\)\s*\{[^}]*\}")
        .wrap_err("Failed to prepare regex")
        .unwrap();

    let new_main_code = if let FunctionRole::Endpoint = function_role {
        format!(
            "use lambda_http::{{run, service_fn, Body, Error, Request, Response, RequestExt}};\n\
            use std::collections::HashMap;\n\
            #[tokio::main]\n\
            async fn main() -> Result<(), Error> {{\n\
                let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                let secrets_client = aws_sdk_secretsmanager::Client::new(&config);
                let secrets_names_env = \"SECRETS_NAMES\";
                let mut secrets = HashMap::new();

                for secret_name in std::env::var(secrets_names_env)?
                    .split(\",\")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {{
                    let secret = secrets_client
                        .get_secret_value()
                        .secret_id(&secret_name)
                        .send()
                        .await?;

                    let secret_value = secret.secret_string().unwrap().to_string();
                    secrets.insert(secret_name, secret_value);
                }}

                run(service_fn(|event| {{
                    {rust_function_name}(event, &secrets)
                }})).await\n\
            }}\n\n\
            {function_code}"
        )
    } else {
        format!(
            "use lambda_runtime::{{LambdaEvent, Error, run, service_fn}};\n\
            use std::collections::HashMap;\n\
            use aws_lambda_events::{{lambda_function_urls::LambdaFunctionUrlRequest, sqs::SqsEvent, sqs::SqsBatchResponse}};\n\n\
            #[tokio::main]\n\
            async fn main() -> Result<(), Error> {{\n\
                run(service_fn(|event| {{
                    {rust_function_name}(event, HashMap::new())
                }})).await\n\
            }}\n\n\
            {function_code}"
        )
    };

    let item: syn::File = syn::parse_str(&new_main_code).unwrap();

    write(
        &main_rs_path,
        re.replace(&source_code, prettyplease::unparse(&item))
            .as_ref(),
    )
    .wrap_err(format!("Failed to write: {main_rs_path:?}"))
    .unwrap();

    // Add [[bin]] section to Cargo.toml
    let cargo_toml_path = dst.join("Cargo.toml");
    let mut cargo_toml_content = read_to_string(&cargo_toml_path).unwrap();

    if !cargo_toml_content.contains("name = \"bootstrap\"") {
        cargo_toml_content.push_str("\n[[bin]]\nname = \"bootstrap\"\npath = \"src/main.rs\"\n");
    }

    if !cargo_toml_content.contains("[package.metadata.sky.function]") {
        cargo_toml_content.push_str(
            format!("\n[package.metadata.sky.function]\nname = \"{function_name}\"\nrole = \"{function_role}\"\nurl_path=\"/some/path\"\n").as_str(),
        );
    }

    if !cargo_toml_content.contains("[package.metadata.sky.environment]") {
        cargo_toml_content.push_str(format!("\n[package.metadata.sky.environment]").as_str());

        for (key, value) in environment.iter() {
            cargo_toml_content.push_str(format!("\n{key} = \"{value}\"").as_str());
        }
    }

    if !cargo_toml_content.contains("aws-config") {
        cargo_toml_content.push_str(
            format!("\n\n[dependencies.aws-config]\nversion=\"1.0.1\"\n").as_str(),
        );
    }

    if !cargo_toml_content.contains("aws-sdk-secretsmanager") {
        cargo_toml_content.push_str(
            format!("\n\n[dependencies.aws-sdk-secretsmanager]\nversion=\"1.20.1\"\n").as_str(),
        );
    }

    write(&cargo_toml_path, &cargo_toml_content)
        .wrap_err(format!("Failed to write: {cargo_toml_path:?}"))
        .unwrap();
}

/// Clean up scaffolding required for deploying a function
fn cleanup(dst: &Path) {
    // Delete the macro attributes from everwhere in the crate
    fn process_files(dir: &Path) -> eyre::Result<()> {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.wrap_err(format!("Failed to read dir: {dir:?}"))?;
            let path = entry.path();

            if path.is_dir() {
                process_files(&path)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                let mut content =
                    read_to_string(&path).wrap_err(format!("Failed to read: {path:?}"))?;

                let re_endpoint = Regex::new(r"(?m)^\s*#\s*\[\s*endpoint[^\]]*\]\s*$")?;
                let re_worker = Regex::new(r"(?m)^\s*#\s*\[\s*worker[^\]]*\]\s*$")?;
                let re_import = Regex::new(
                    r"(?m)^\s*use\s+skymacro(\s*::\s*(\w+|\{\s*\w+(\s*,\s*\w+)*\s*\}))?\s*;\s*$",
                )?;
                content = re_endpoint.replace_all(&content, "").to_string();
                content = re_worker.replace_all(&content, "").to_string();
                let new_content = re_import.replace_all(&content, "");

                write(&path, new_content.as_ref())
                    .wrap_err(format!("Failed to write: {path:?}"))?;
            }
        }
        Ok(())
    }

    process_files(&dst.join("src")).unwrap();

    // Delete procmacro crate from [dependencies] in Cargo.toml
    let cargo_toml_path = dst.join("Cargo.toml");

    let cargo_toml_content = read_to_string(&cargo_toml_path)
        .wrap_err(format!("Failed to read: {cargo_toml_path:?}"))
        .unwrap();

    let mut cargo_toml_value: toml::Value = cargo_toml_content
        .parse()
        .wrap_err(format!(
            "Failed to parse Cargo.toml at: {cargo_toml_path:?}"
        ))
        .unwrap();

    // Delete the resources definitions. This is the general list, each function gets its own.
    cargo_toml_value
        .get_mut("package")
        .wrap_err("No [package]")
        .unwrap()
        .get_mut("metadata")
        .wrap_err("No [metadata]")
        .unwrap()
        .as_table_mut()
        .unwrap()
        .remove("sky");

    if let Some(dependencies) = cargo_toml_value
        .get_mut("dependencies")
        .and_then(|d| d.as_table_mut())
    {
        dependencies.remove("skymacro");
    }

    write(
        &cargo_toml_path,
        toml::to_string(&cargo_toml_value).unwrap(),
    )
    .unwrap();
}

pub fn process_function(attr: TokenStream, item: TokenStream, role: FunctionRole) -> TokenStream {
    let attrs = Attrs::new(attr, &role)
        .wrap_err("Failed to parse attributes")
        .unwrap();

    let span = proc_macro::Span::call_site();
    let source_file = span.source_file();
    let item_fn = item.clone();

    // Extract the function name
    let ast: ItemFn = parse_macro_input!(item_fn as ItemFn);
    let rust_name = &ast.sig.ident.to_string();

    let lambda_name = func_name(&attrs, &source_file, &rust_name)
        .wrap_err("Failed to generate function name")
        .unwrap();

    let src_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let src = Path::new(&src_dir);

    let dst = skypath()
        .unwrap()
        .join(std::env::var("CARGO_CRATE_NAME").unwrap())
        .join(&lambda_name);

    clone(src, &dst);

    // Must be called before inject() and resources(), to avoid deleting the code these two functions add
    cleanup(&dst);

    inject(
        &dst,
        &lambda_name.to_string(),
        &rust_name,
        &item.to_string(),
        role,
        &attrs.environment(),
    );

    item
}
