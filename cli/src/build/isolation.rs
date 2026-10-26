use super::rust_paths::{self, function_path, module_path};
use kinetics_parser::{ParsedFunction, ParsedPackage};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use syn::visit::Visit;
use syn::Item;

/// Handlers from one source file and whether their helpers can move with them.
pub(super) struct HandlerModule<'a> {
    pub names: BTreeSet<&'a str>,
    pub move_helpers: bool,
    targets: Vec<Vec<String>>,
}

/// Select handler files that can leave the shared library without breaking references.
pub(super) fn select_modules<'a>(
    sources: &BTreeMap<PathBuf, syn::File>,
    functions: &'a [ParsedFunction],
    lib_path: &Path,
    lib_name: &str,
    external_sources: &[(&ParsedPackage, &str)],
    private_macros: &BTreeSet<String>,
) -> BTreeMap<PathBuf, HandlerModule<'a>> {
    let mut groups: BTreeMap<PathBuf, HandlerModule<'_>> = BTreeMap::new();
    for function in functions {
        let group = groups
            .entry(function.relative_path.clone())
            .or_insert_with(|| HandlerModule {
                names: BTreeSet::new(),
                move_helpers: false,
                targets: Vec::new(),
            });
        group.names.insert(&function.rust_function_name);
        group.targets.push(function_path(function, lib_path));
    }
    for (path, group) in &mut groups {
        group.move_helpers =
            path != lib_path && can_move_helpers(&sources[path].items, &group.names);
    }
    let mut retained = BTreeMap::new();
    for (path, syntax) in sources {
        let group = groups.get(path);
        let mut references = References {
            module: module_path(path, lib_path),
            lib_name: lib_name.to_owned(),
            check_self_reference: group.is_none_or(|group| !group.move_helpers),
            own_handlers: group
                .into_iter()
                .flat_map(|group| &group.names)
                .map(|name| (*name).to_owned())
                .collect(),
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
        retain_referenced(&mut retained, &references, &groups, Some(path));
    }
    for (package, dependency_name) in external_sources {
        for syntax in package.sources.values() {
            let mut references = References {
                external_prefixes: vec![(*dependency_name).to_owned(), lib_name.to_owned()],
                ..Default::default()
            };
            references.visit_file(syntax);
            retain_referenced(&mut retained, &references, &groups, None);
        }
    }
    groups.retain(|path, _| {
        if let Some(reason) = retained.get(path) {
            log::debug!(
                "Keeping handler module {} in shared library: {reason}",
                path.display()
            );
            false
        } else {
            log::debug!("Isolated handler module {}", path.display());
            true
        }
    });
    groups
}

/// Keep handler files that another module or workspace package references.
fn retain_referenced(
    retained: &mut BTreeMap<PathBuf, &'static str>,
    references: &References,
    groups: &BTreeMap<PathBuf, HandlerModule<'_>>,
    own_path: Option<&Path>,
) {
    for (path, group) in groups {
        if own_path == Some(path.as_path()) {
            continue;
        }
        if references.paths.iter().any(|(reference, namespace)| {
            group
                .targets
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

/// Allow helpers to move only when the file exposes no other shared items.
fn can_move_helpers(items: &[Item], handlers: &BTreeSet<&str>) -> bool {
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

/// Resolved references and constraints that can prevent handler isolation.
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
    /// Resolve a path and flag references that require the original module.
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
        let resolved = rust_paths::resolve(&parts, &self.module, &self.lib_name);
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
    /// Inspect opaque macro tokens and retain files that need their original context.
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

    /// Allow handler-to-handler calls within a file that will move together.
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        let previous = self.check_self_reference;
        if self.own_handlers.contains(&function.sig.ident.to_string()) {
            self.check_self_reference = false;
        }
        syn::visit::visit_item_fn(self, function);
        self.check_self_reference = previous;
    }

    /// Keep unresolved from_worker calls in their original module to preserve type identity.
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if matches!(&*call.func, syn::Expr::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "from_worker"))
        {
            // Unknown aliases/expressions keep the original function type identity.
            self.keep_shared = Some("reference requires the original module");
        }
        syn::visit::visit_expr_call(self, call);
    }

    /// Collect referenced paths, including those used in types and signatures.
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

    /// Also check value references for shared helpers that call their own handlers.
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

    /// Treat imported namespaces conservatively as references to their handlers.
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        rust_paths::visit_imports(&item.tree, &mut |path, _| self.record(path, true, false));
    }

    /// Resolve inline module contents in their nested namespace.
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
