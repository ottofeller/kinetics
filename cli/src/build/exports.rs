use super::rust_paths::{self, module_path};
use kinetics_parser::ParsedFunction;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::{Item, UseTree};

/// Collect non-exported macro names whose imports must stay private.
pub(super) fn collect_private_macros(sources: &BTreeMap<PathBuf, syn::File>) -> BTreeSet<String> {
    sources
        .values()
        .flat_map(|file| &file.items)
        .filter_map(|item| match item {
            Item::Macro(item)
                if !item
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("macro_export")) =>
            {
                item.ident.as_ref().map(ToString::to_string)
            }
            _ => None,
        })
        .collect()
}

/// Make shared items and handler module paths accessible from generated binaries.
pub(super) fn expose_to_binaries(
    sources: &mut BTreeMap<PathBuf, syn::File>,
    functions: &[ParsedFunction],
    lib_path: &Path,
    private_macros: &BTreeSet<String>,
) -> eyre::Result<()> {
    for file in sources.values_mut() {
        expose_items(&mut file.items, private_macros);
    }
    export_modules(
        &mut sources
            .get_mut(lib_path)
            .expect("parsed library exists")
            .items,
        functions,
        lib_path,
    )
}

/// Declare or export top-level modules containing discovered handlers.
fn export_modules(
    items: &mut Vec<Item>,
    functions: &[ParsedFunction],
    lib_path: &Path,
) -> eyre::Result<()> {
    for function in functions {
        let modules = module_path(&function.relative_path, lib_path);
        let Some(name) = modules.first() else {
            continue;
        };
        if let Some(module) = items.iter_mut().find_map(|item| match item {
            Item::Mod(module) if module.ident == name => Some(module),
            _ => None,
        }) {
            module.vis = syn::parse_quote!(pub);
        } else {
            let name: syn::Ident = syn::parse_str(name)?;
            items.push(syn::parse_quote!(pub mod #name;));
        }
    }
    Ok(())
}

/// Detect imports of known non-exported macros.
fn has_private_macro_import(tree: &UseTree, names: &BTreeSet<String>) -> bool {
    let mut has_private_macro = false;
    rust_paths::visit_imports(tree, &mut |path, _| {
        has_private_macro |= path.last().is_some_and(|name| names.contains(name));
    });
    has_private_macro
}

/// Promote item visibility while preserving private macro imports and trait impls.
fn expose_items(items: &mut [Item], private_macros: &BTreeSet<String>) {
    for item in items {
        // Binaries are separate crates. Preserve access that handler child modules
        // previously had to their parent modules, types and inherent methods.
        match item {
            Item::Const(item) => item.vis = syn::parse_quote!(pub),
            Item::Enum(item) => item.vis = syn::parse_quote!(pub),
            Item::ExternCrate(item) => item.vis = syn::parse_quote!(pub),
            Item::Fn(item) => item.vis = syn::parse_quote!(pub),
            Item::Mod(item) => {
                item.vis = syn::parse_quote!(pub);
                if let Some((_, items)) = &mut item.content {
                    expose_items(items, private_macros);
                }
            }
            Item::Static(item) => item.vis = syn::parse_quote!(pub),
            Item::Struct(item) => {
                item.vis = syn::parse_quote!(pub);
                for field in &mut item.fields {
                    field.vis = syn::parse_quote!(pub);
                }
            }
            Item::Trait(item) => item.vis = syn::parse_quote!(pub),
            Item::TraitAlias(item) => item.vis = syn::parse_quote!(pub),
            Item::Type(item) => item.vis = syn::parse_quote!(pub),
            Item::Union(item) => {
                item.vis = syn::parse_quote!(pub);
                for field in &mut item.fields.named {
                    field.vis = syn::parse_quote!(pub);
                }
            }
            Item::Use(item) if !has_private_macro_import(&item.tree, private_macros) => {
                item.vis = syn::parse_quote!(pub)
            }
            Item::Impl(item) if item.trait_.is_none() => {
                for item in &mut item.items {
                    match item {
                        syn::ImplItem::Fn(item) => item.vis = syn::parse_quote!(pub),
                        syn::ImplItem::Const(item) => item.vis = syn::parse_quote!(pub),
                        syn::ImplItem::Type(item) => item.vis = syn::parse_quote!(pub),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}
