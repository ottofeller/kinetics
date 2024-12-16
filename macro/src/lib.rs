#![feature(proc_macro_totokens, proc_macro_span)]
use eyre::WrapErr;
use proc_macro::TokenStream;
use regex::Regex;
use std::fs::read_to_string;
use std::fs::write;
use std::path::Path;
use syn::{parse_macro_input, ItemFn};

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

/// Inject the code which is necessary to build lambda
///
/// Set up the main() function according to cargo lambda guides, and add the lambda code right to main.rs
fn inject(dst: &Path, function_name: &str, function_code: &str) {
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
            run(service_fn({function_name})).await\n\
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
                let content =
                    read_to_string(&path).wrap_err(format!("Failed to read: {path:?}"))?;

                let re_attr = Regex::new(r"(?m)^\s*#\s*\[\s*lambda[^\]]*\]\s*$")?;
                let re_import = Regex::new(r"(?m)^\s*use\s+procmacro(\s*::\s*\w+)?\s*;\s*$")?;
                let new_content = re_attr.replace_all(&content, "");
                let new_content = re_import.replace_all(&new_content, "");

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

    let re = Regex::new(r"(?m)^\s*procmacro\s*=.*\n?").unwrap();
    let new_cargo_toml_content = re.replace_all(&cargo_toml_content, "");
    write(&cargo_toml_path, new_cargo_toml_content.as_ref()).unwrap();
}

/// Deploy a function to the cloud
#[proc_macro_attribute]
pub fn endpoint(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let span = proc_macro::Span::call_site();
    let source_file = span.source_file();
    let item_fn = item.clone();

    // Extract the function name
    let ast: ItemFn = parse_macro_input!(item_fn as ItemFn);
    let func_name = &ast.sig.ident;

    let src_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let src = Path::new(&src_dir);

    // Generate a unique project directory name
    let project_dir = format!(
        "{}-{}",
        source_file.path().to_string_lossy().replace("/", "-"),
        func_name
    );

    let dst = Path::new(&std::env::var("HOME").unwrap())
        .join(".sky")
        .join(std::env::var("CARGO_CRATE_NAME").unwrap())
        .join(project_dir);

    clone(src, &dst);
    inject(&dst, &func_name.to_string(), &item.to_string());
    cleanup(&dst);
    item
}
