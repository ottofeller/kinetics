use super::paths::{self, module_path};
use kinetics_parser::{ParsedFunction, ParsedPackage, Role};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use syn::visit::Visit;
use syn::{Item, UseTree};

/// Generated sources, independent of the filesystem and Cargo invocation.
pub(super) struct PreparedPackage {
    pub shared: BTreeMap<PathBuf, syn::File>,
    pub handlers: BTreeMap<PathBuf, syn::File>,
    lib_name: String,
    lib_path: PathBuf,
}

impl PreparedPackage {
    pub fn new(
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
            super::queue_references::lower(
                syntax,
                &module_path(path, lib_path),
                &workers,
                &lib_name,
            );
        }
        let private_macros = shared
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
            .collect();
        let mut groups: BTreeMap<PathBuf, BTreeSet<&str>> = BTreeMap::new();
        for function in functions {
            groups
                .entry(function.relative_path.clone())
                .or_default()
                .insert(&function.rust_function_name);
        }
        let candidates: BTreeMap<_, _> = groups
            .keys()
            .map(|path| {
                let targets = functions
                    .iter()
                    .filter(|function| function.relative_path == *path)
                    .map(|function| function_path(function, lib_path))
                    .collect::<Vec<_>>();
                (path.clone(), targets)
            })
            .collect();
        let retained = retained_handlers(
            &shared,
            &groups,
            &candidates,
            lib_path,
            &lib_name,
            external_sources,
            &private_macros,
        );
        let mut handlers = BTreeMap::new();
        for (path, names) in &groups {
            if let Some(reason) = retained.get(path) {
                log::debug!(
                    "Keeping handler module {} in shared library: {reason}",
                    path.display()
                );
                continue;
            }
            log::debug!("Isolated handler module {}", path.display());
            let mut file = shared[path].clone();
            let local_helpers = path != lib_path && helpers_are_local(&file.items, names);
            file.items.retain(|item| {
                local_helpers
                    || match item {
                        Item::Use(_) | Item::ExternCrate(_) => true,
                        Item::Fn(item) => names.contains(item.sig.ident.to_string().as_str()),
                        Item::Macro(item) => item.ident.is_some(),
                        _ => false,
                    }
            });
            let module = module_path(path, lib_path);
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
            handlers.insert(path.clone(), file);
            shared.get_mut(path).expect("handler source exists").items.retain(|item| {
                if local_helpers {
                    false
                } else {
                    !matches!(item, Item::Fn(item) if names.contains(item.sig.ident.to_string().as_str()))
                }
            });
        }
        for file in shared.values_mut() {
            prepare_shared(&mut file.items, &private_macros);
        }
        export_modules(
            &mut shared
                .get_mut(lib_path)
                .expect("parsed library exists")
                .items,
            functions,
            lib_path,
        )?;
        Ok(Self {
            shared,
            handlers,
            lib_name,
            lib_path: lib_path.to_path_buf(),
        })
    }

    pub fn binary_import(&self, root: &Path, function: &ParsedFunction) -> String {
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

fn function_path(function: &ParsedFunction, lib_path: &Path) -> Vec<String> {
    let mut path = module_path(&function.relative_path, lib_path);
    path.push(function.rust_function_name.clone());
    path
}

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

fn retain_referenced(
    retained: &mut BTreeMap<PathBuf, &'static str>,
    references: &References,
    candidates: &BTreeMap<PathBuf, Vec<Vec<String>>>,
    own_path: Option<&Path>,
) {
    for (path, targets) in candidates {
        if own_path == Some(path.as_path()) {
            continue;
        }
        if references.paths.iter().any(|(reference, namespace)| {
            targets
                .iter()
                .any(|target| reference == target || (*namespace && target.starts_with(reference)))
        }) {
            retained.insert(
                path.clone(),
                "handler is referenced by another module or package",
            );
        }
    }
}

fn retained_handlers(
    sources: &BTreeMap<PathBuf, syn::File>,
    groups: &BTreeMap<PathBuf, BTreeSet<&str>>,
    candidates: &BTreeMap<PathBuf, Vec<Vec<String>>>,
    lib_path: &Path,
    lib_name: &str,
    external_sources: &[(&ParsedPackage, &str)],
    private_macros: &BTreeSet<String>,
) -> BTreeMap<PathBuf, &'static str> {
    let mut retained = BTreeMap::new();
    for (path, syntax) in sources {
        let names = groups.get(path).cloned().unwrap_or_default();
        let mut references = References {
            module: module_path(path, lib_path),
            lib_name: lib_name.to_owned(),
            check_self_reference: path == lib_path || !helpers_are_local(&syntax.items, &names),
            own_handlers: names.iter().map(|name| (*name).to_owned()).collect(),
            private_macros: private_macros.clone(),
            own_macros: syntax
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Macro(item) => item.ident.as_ref().map(ToString::to_string),
                    _ => None,
                })
                .collect(),
            ..Default::default()
        };
        references.visit_file(syntax);
        if let Some(reason) = references.keep_shared {
            retained.insert(path.clone(), reason);
        }
        if syntax
            .items
            .iter()
            .any(|item| matches!(item, Item::Macro(item) if item.ident.is_none()))
        {
            retained.insert(path.clone(), "module contains a macro-generated item");
        }
        retain_referenced(&mut retained, &references, candidates, Some(path));
    }
    for (package, dependency_name) in external_sources {
        for syntax in package.sources.values() {
            let mut references = References {
                external_prefixes: vec![(*dependency_name).to_owned(), lib_name.to_owned()],
                ..Default::default()
            };
            references.visit_file(syntax);
            retain_referenced(&mut retained, &references, candidates, None);
        }
    }
    retained
}

fn helpers_are_local(items: &[Item], handlers: &BTreeSet<&str>) -> bool {
    let private_types: BTreeSet<_> = items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item) if matches!(item.vis, syn::Visibility::Inherited) => {
                Some(item.ident.to_string())
            }
            Item::Enum(item) if matches!(item.vis, syn::Visibility::Inherited) => {
                Some(item.ident.to_string())
            }
            Item::Union(item) if matches!(item.vis, syn::Visibility::Inherited) => {
                Some(item.ident.to_string())
            }
            _ => None,
        })
        .collect();
    items.iter().all(|item| match item {
        Item::Fn(item) => handlers.contains(item.sig.ident.to_string().as_str()) || matches!(item.vis, syn::Visibility::Inherited),
        Item::Const(item) => matches!(item.vis, syn::Visibility::Inherited),
        Item::Static(item) => matches!(item.vis, syn::Visibility::Inherited),
        Item::Struct(item) => matches!(item.vis, syn::Visibility::Inherited),
        Item::Enum(item) => matches!(item.vis, syn::Visibility::Inherited),
        Item::Union(item) => matches!(item.vis, syn::Visibility::Inherited),
        Item::Type(item) => matches!(item.vis, syn::Visibility::Inherited),
        Item::Trait(item) => matches!(item.vis, syn::Visibility::Inherited),
        Item::Use(item) => matches!(item.vis, syn::Visibility::Inherited),
        Item::ExternCrate(item) => matches!(item.vis, syn::Visibility::Inherited),
        Item::Impl(item) => matches!(&*item.self_ty, syn::Type::Path(path) if path.qself.is_none()
            && path.path.get_ident().is_some_and(|ident| private_types.contains(&ident.to_string()))),
        Item::Macro(item) => item.ident.is_some() && !item.attrs.iter().any(|attr| attr.path().is_ident("macro_export")),
        _ => false,
    })
}

fn imports_private_macro(tree: &UseTree, names: &BTreeSet<String>) -> bool {
    let mut found = false;
    paths::visit_imports(tree, &mut |path, _| {
        found |= path.last().is_some_and(|name| names.contains(name));
    });
    found
}

fn prepare_shared(items: &mut [Item], private_macros: &BTreeSet<String>) {
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
                    prepare_shared(items, private_macros);
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
            Item::Use(item) if !imports_private_macro(&item.tree, private_macros) => {
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

#[derive(Default)]
struct References {
    module: Vec<String>,
    lib_name: String,
    own_handlers: BTreeSet<String>,
    paths: Vec<(Vec<String>, bool)>,
    keep_shared: Option<&'static str>,
    check_self_reference: bool,
    external_prefixes: Vec<String>,
    private_macros: BTreeSet<String>,
    own_macros: BTreeSet<String>,
}

impl References {
    fn record(&mut self, parts: Vec<String>, namespace: bool, expression: bool) {
        if parts.is_empty() {
            return;
        }
        if !self.external_prefixes.is_empty() {
            if self.external_prefixes.contains(&parts[0]) {
                self.paths.push((parts[1..].to_vec(), namespace));
            }
            return;
        }
        let resolved = paths::resolve(&parts, &self.module, &self.lib_name);
        if resolved == ["main"] || resolved == ["__kinetics_user_function"] {
            self.keep_shared = Some("reference requires the original module");
        }
        if self.check_self_reference
            && expression
            && resolved.len() == self.module.len() + 1
            && resolved.starts_with(&self.module)
            && resolved
                .last()
                .is_some_and(|name| self.own_handlers.contains(name))
        {
            self.keep_shared = Some("reference requires the original module");
        }
        self.paths.push((resolved, namespace));
    }
}

impl<'ast> Visit<'ast> for References {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if mac.path.segments.last().is_some_and(|segment| {
            let name = segment.ident.to_string();
            self.private_macros.contains(&name) && !self.own_macros.contains(&name)
        }) {
            self.keep_shared = Some("reference requires the original module");
        }
        if mac.path.segments.last().is_some_and(|segment| {
            matches!(
                segment.ident.to_string().as_str(),
                "include" | "include_str" | "include_bytes"
            )
        }) {
            self.keep_shared = Some("relative include requires the original source directory");
        }
        static QUALIFIED_PATH: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r"(?:[A-Za-z_][A-Za-z_0-9]*\s*::\s*)+[A-Za-z_][A-Za-z_0-9]*")
                .expect("valid qualified path regex")
        });
        let tokens = mac.tokens.to_string();
        if tokens.contains("from_worker") {
            self.keep_shared = Some("reference requires the original module");
        }
        for path in QUALIFIED_PATH.find_iter(&tokens) {
            self.record(
                path.as_str()
                    .split("::")
                    .map(|part| part.trim().to_owned())
                    .collect(),
                false,
                true,
            );
        }
        syn::visit::visit_macro(self, mac);
    }

    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        let previous = self.check_self_reference;
        if self.own_handlers.contains(&function.sig.ident.to_string()) {
            self.check_self_reference = false;
        }
        syn::visit::visit_item_fn(self, function);
        self.check_self_reference = previous;
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if matches!(&*call.func, syn::Expr::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "from_worker"))
        {
            // Unknown aliases/expressions keep the original function type identity.
            self.keep_shared = Some("reference requires the original module");
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.record(
            path.segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect(),
            false,
            false,
        );
        syn::visit::visit_path(self, path);
    }

    fn visit_expr_path(&mut self, expression: &'ast syn::ExprPath) {
        self.record(
            expression
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect(),
            false,
            true,
        );
        syn::visit::visit_expr_path(self, expression);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        paths::visit_imports(&item.tree, &mut |path, _| self.record(path, true, false));
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if let Some((_, items)) = &item.content {
            self.module.push(item.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.module.pop();
        }
    }
}
