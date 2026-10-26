use super::rust_paths::{function_path, module_path};
use super::{exports, isolation, queue_calls};
use kinetics_parser::{ParsedFunction, ParsedPackage, Role};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use syn::Item;

/// Generated sources, independent of the filesystem and Cargo invocation.
pub(super) struct PackageSources {
    pub shared: BTreeMap<PathBuf, syn::File>,
    pub handlers: BTreeMap<PathBuf, syn::File>,
    lib_name: String,
    lib_path: PathBuf,
}

impl PackageSources {
    /// Rewrite queue calls, extract eligible handlers, and expose the shared library.
    pub fn generate(
        parsed: &ParsedPackage,
        lib_path: &Path,
        lib_name: String,
        external_sources: &[(&ParsedPackage, &str)],
    ) -> eyre::Result<Self> {
        let functions = &parsed.functions;
        let mut shared = parsed.sources.clone();
        let workers = functions
            .iter()
            .filter(|function| matches!(function.role, Role::Worker | Role::DlqWorker))
            .map(|function| function_path(function, lib_path))
            .collect();
        for (path, syntax) in &mut shared {
            queue_calls::replace_from_worker(
                syntax,
                &module_path(path, lib_path),
                &workers,
                &lib_name,
            );
        }
        let private_macros = exports::collect_private_macros(&shared);
        let isolated = isolation::select_modules(
            &shared,
            functions,
            lib_path,
            &lib_name,
            external_sources,
            &private_macros,
        );
        let mut handlers = BTreeMap::new();
        for (path, group) in isolated {
            let names = &group.names;
            let original = shared.get_mut(&path).expect("handler source exists");
            let mut file = original.clone();
            file.items.retain(|item| {
                group.move_helpers
                    || match item {
                        Item::Use(_) | Item::ExternCrate(_) => true,
                        Item::Fn(item) => names.contains(item.sig.ident.to_string().as_str()),
                        Item::Macro(item) => item.ident.is_some(),
                        _ => false,
                    }
            });
            let module = module_path(&path, lib_path);
            let namespace = std::iter::once(lib_name.to_owned())
                .chain(module)
                .collect::<Vec<_>>()
                .join("::");
            file.items.insert(
                0,
                syn::parse_str(&format!("#[allow(unused_imports)] use {namespace}::*;"))?,
            );
            for item in &mut file.items {
                match item {
                    Item::Fn(item) if names.contains(item.sig.ident.to_string().as_str()) => {
                        item.vis = syn::parse_quote!(pub)
                    }
                    Item::Use(item) => item.attrs.push(syn::parse_quote!(#[allow(unused_imports)])),
                    _ => {}
                }
            }
            handlers.insert(path, file);
            if group.move_helpers {
                original.items.clear();
            } else {
                original.items.retain(|item| {
                    !matches!(item, Item::Fn(item) if names.contains(item.sig.ident.to_string().as_str()))
                });
            }
        }
        exports::expose_to_binaries(&mut shared, functions, lib_path, &private_macros)?;
        Ok(Self {
            shared,
            handlers,
            lib_name,
            lib_path: lib_path.to_path_buf(),
        })
    }

    /// Render the import from either the shared library or an isolated handler file.
    pub fn render_handler_import(&self, root: &Path, function: &ParsedFunction) -> String {
        let lib_name = &self.lib_name;
        let modules = module_path(&function.relative_path, &self.lib_path);
        if !self.handlers.contains_key(&function.relative_path) {
            let mut path = vec![lib_name.clone()];
            path.extend(modules);
            path.push(function.rust_function_name.clone());
            return format!("use {} as __kinetics_user_function;", path.join("::"));
        }
        let leaf = modules
            .last()
            .map(String::as_str)
            .unwrap_or("__kinetics_handler");
        let source = root
            .join(".kinetics/handlers")
            .join(&function.relative_path);
        let mut code = format!("#[path = {source:?}] pub mod {leaf};");
        for depth in (1..modules.len()).rev() {
            let name = &modules[depth - 1];
            let shared_module = modules[..depth].join("::");
            code = format!(
                "pub mod {name} {{ #[allow(unused_imports)] pub use {lib_name}::{shared_module}::*; {code} }}"
            );
        }
        let import = if modules.is_empty() {
            "__kinetics_handler".to_owned()
        } else {
            modules.join("::")
        };
        format!(
            "#[allow(unused_imports)] pub use {lib_name}::*;\n{code}\nuse self::{}::{} as __kinetics_user_function;",
            import, function.rust_function_name,
        )
    }
}
