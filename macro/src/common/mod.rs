use crate::FunctionRole;
use eyre::{eyre, ContextCompat, WrapErr};
use proc_macro::TokenStream;
use proc_macro::{SourceFile, ToTokens};
use regex::Regex;
use std::collections::HashMap;
use std::fs::read_to_string;
use std::fs::write;
use std::path::Path;
use std::path::PathBuf;
use syn::{parse_macro_input, ItemFn, LitInt, LitStr};

/// Where all the projects are copied
fn skypath() -> eyre::Result<PathBuf> {
    Ok(Path::new(&std::env::var("HOME").wrap_err("Can not read HOME env var")?).join(".sky"))
}

/// Parse the macro input attributes into a hashmap
fn attrs(input: TokenStream) -> eyre::Result<HashMap<String, String>> {
    let mut result = HashMap::<String, String>::new();
    let mut name = String::new();

    for token in input.into_iter() {
        match token {
            proc_macro::TokenTree::Ident(ident) => name = ident.to_string(),

            proc_macro::TokenTree::Literal(literal) => {
                // Try to parse inptut as a string literal first
                let token_stream = literal.to_token_stream();
                let try_str = syn::parse::<LitStr>(token_stream.clone());
                let try_int = syn::parse::<LitInt>(token_stream);

                if try_str.is_err() && try_int.is_err() {
                    return Err(eyre!(
                        "The input attr has unsupported format (should be str or int)"
                    ));
                }

                if try_str.is_ok() {
                    result.insert(name.clone(), try_str?.value());
                } else {
                    result.insert(name.clone(), try_int?.to_string());
                }
            }

            _ => {}
        }
    }

    Ok(result)
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
fn func_name(
    attrs: &HashMap<String, String>,
    file: &SourceFile,
    rust_name: &str,
) -> eyre::Result<String> {
    let default_func_name = &format!(
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
    Ok(attrs.get("name").unwrap_or(default_func_name).to_string())
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
) {
    let main_rs_path = dst.join("src").join("main.rs");

    let source_code = read_to_string(&main_rs_path)
        .wrap_err("Reading main.rs failed")
        .unwrap();

    let re = Regex::new(r"fn\s+main\s*\(.*?\)\s*\{[^}]*\}")
        .wrap_err("Failed to prepare regex")
        .unwrap();

    let new_main_code = format!(
        "use lambda_http::{{run, service_fn, tracing, Body, Error, Request, RequestExt, Response}};\n\n\
        #[tokio::main]\n\
        async fn main() -> Result<(), Error> {{\n\
            tracing::init_default_subscriber();\n\
            run(service_fn({rust_function_name})).await\n\
        }}\n\n\
        {function_code}"
    );

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

    if !cargo_toml_content.contains("[[metadata.sky.function]]") {
        cargo_toml_content.push_str(
            format!("\n[[metadata.sky.function]]\nname = \"{function_name}\"\nrole = \"{function_role}\"\nurl_path=\"/some/path\"\n").as_str(),
        );
    }

    write(&cargo_toml_path, &cargo_toml_content)
        .wrap_err(format!("Failed to write: {cargo_toml_path:?}"))
        .unwrap();
}

/// Clean up scaffolding required for deploying a function
///
/// This is done to avoid infinite loop caused by the macro attributes being executed every time a crate is copied.
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
                let re_import = Regex::new(r"(?m)^\s*use\s+skymacro(\s*::\s*\w+)?\s*;\s*$")?;
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

    let re = Regex::new(r"(?m)^\s*skymacro\s*=.*\n?").unwrap();
    let new_cargo_toml_content = re.replace_all(&cargo_toml_content, "");
    write(&cargo_toml_path, new_cargo_toml_content.as_ref()).unwrap();
}

// TODO Handle this in a nicer way with traits (for example endpoint does not need a queue)
/// Copy relevant resources definitions from source Cargo.toml to Cargo.toml in the target directory
pub fn resources(dst: &PathBuf, attrs: &HashMap<String, String>) -> eyre::Result<()> {
    let src_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let src = Path::new(&src_dir);
    let src_cargo_toml_path = src.join("Cargo.toml");
    let dst_cargo_toml_path = dst.join("Cargo.toml");

    let src_cargo_toml: toml::Value = std::fs::read_to_string(src_cargo_toml_path)
        .wrap_err("Failed to read Cargo.toml: {cargo_toml_path:?}")?
        .parse::<toml::Value>()
        .wrap_err("Failed to parse Cargo.toml")?;

    let mut dst_cargo_toml_content = read_to_string(&dst_cargo_toml_path)
        .wrap_err(format!("Failed to read: {dst_cargo_toml_path:?}"))?;

    for (key, value) in attrs
        .iter()
        .filter(|(k, _)| vec!["queue", "db"].contains(&k.as_str()))
    {
        let name = value;
        let head = format!("[[metadata.sky.{key}.{name}]]");

        let body = src_cargo_toml
            .get("metadata")
            .wrap_err("No [metadata]")?
            .get("sky")
            .wrap_err("No [sky]")?
            .get(key)
            .wrap_err(format!("No {key}"))?
            .get(value)
            .wrap_err(format!("No {value}"))?
            .to_string();

        if !dst_cargo_toml_content.contains(&head) {
            dst_cargo_toml_content.push_str("\n");
            dst_cargo_toml_content.push_str(&format!("{head}\n{body}"));

            write(&dst_cargo_toml_path, &dst_cargo_toml_content)
                .wrap_err(format!("Failed to write: {dst_cargo_toml_path:?}"))?;
        }
    }

    Ok(())
}

pub fn process_function(attr: TokenStream, item: TokenStream, role: FunctionRole) -> TokenStream {
    let attrs = attrs(attr).wrap_err("Failed to parse attributes").unwrap();
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

    inject(
        &dst,
        &lambda_name.to_string(),
        &rust_name,
        &item.to_string(),
        role,
    );

    resources(&dst, &attrs).unwrap();
    cleanup(&dst);
    item
}
