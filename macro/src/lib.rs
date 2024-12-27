#![feature(proc_macro_totokens, proc_macro_span, proc_macro_expand)]
mod common;
use eyre::WrapErr;
use proc_macro::TokenStream;
use std::fmt::Display;
use std::fs::read_to_string;
use std::fs::write;
use syn::{parse_macro_input, ItemFn};

enum FunctionRole {
    Endpoint,
    Worker,
}

impl Display for FunctionRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FunctionRole::Endpoint => write!(f, "endpoint"),
            FunctionRole::Worker => write!(f, "worker"),
        }
    }
}

#[proc_macro_attribute]
pub fn endpoint(attr: TokenStream, item: TokenStream) -> TokenStream {
    common::process_function(attr, item, FunctionRole::Endpoint)
}

#[proc_macro_attribute]
pub fn worker(attr: TokenStream, item: TokenStream) -> TokenStream {
    let result = common::process_function(attr.clone(), item.clone(), FunctionRole::Worker);

    // Create queue resource in the dst Cargo.toml based on the input of the macro
    let attrs = common::attrs(attr)
        .wrap_err("Failed to parse attributes")
        .unwrap();

    let span = proc_macro::Span::call_site();
    let source_file = span.source_file();
    let item_fn = item.clone();
    let ast: ItemFn = parse_macro_input!(item_fn as ItemFn);
    let rust_name = &ast.sig.ident.to_string();

    let lambda_name = common::func_name(&attrs, &source_file, &rust_name)
        .wrap_err("Failed to generate function name")
        .unwrap();

    let dst = common::skypath()
        .unwrap()
        .join(std::env::var("CARGO_CRATE_NAME").unwrap())
        .join(&lambda_name);

    let dst_cargo_toml_path = dst.join("Cargo.toml");

    let mut dst_cargo_toml_content = read_to_string(&dst_cargo_toml_path)
        .wrap_err(format!("Failed to read dst Cargo.toml"))
        .unwrap();

    dst_cargo_toml_content.push_str("\n");

    dst_cargo_toml_content.push_str(&format!(
        "[package.metadata.sky.queue.{}]\nname=\"{}\"\nconcurrency={}\nfifo={}",
        attrs.get("name").unwrap_or(&lambda_name),
        attrs.get("name").unwrap_or(&lambda_name),
        attrs.get("concurrency").unwrap_or(&1.to_string()),
        attrs.get("fifo").unwrap_or(&false.to_string()),
    ));

    write(&dst_cargo_toml_path, &dst_cargo_toml_content)
        .wrap_err(format!("Failed to write: {dst_cargo_toml_path:?}"))
        .unwrap();

    result
}
