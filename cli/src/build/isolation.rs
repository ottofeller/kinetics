use super::rust_paths::{self, function_path, module_path};
use kinetics_parser::{ParsedFunction, ParsedPackage};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use syn::buffer::{Cursor, TokenBuffer};
use syn::visit::Visit;
use syn::Item;

/// Handlers from one source file and whether their helpers can move with them.
pub(super) struct HandlerModule<'a> {
    pub names: BTreeSet<&'a str>,
    pub is_helper_move_allowed: bool,
    targets: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Copy)]
enum IsolationBlockReason {
    GeneratedNameConflict,
    SharedCodeReference,
    PrivateMacro,
    RelativeInclude,
    WorkerReferenceInMacro,
    UnresolvedWorkerCall,
    MacroGeneratedItem,
    CrossModuleHandlerReference,
}

impl fmt::Display for IsolationBlockReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::GeneratedNameConflict => "reference conflicts with generated binary names",
            Self::SharedCodeReference => "shared code references a handler in the same module",
            Self::PrivateMacro => "macro is defined privately outside this module",
            Self::RelativeInclude => "relative include requires the original source directory",
            Self::WorkerReferenceInMacro => "macro contains an unresolved from_worker reference",
            Self::UnresolvedWorkerCall => {
                "unresolved from_worker call requires the original worker type"
            }
            Self::MacroGeneratedItem => "module contains a macro-generated item",
            Self::CrossModuleHandlerReference => {
                "handler is referenced by another module or package"
            }
        })
    }
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
                is_helper_move_allowed: false,
                targets: Vec::new(),
            });
        group.names.insert(&function.rust_function_name);
        group.targets.push(function_path(function, lib_path));
    }
    for (path, group) in &mut groups {
        group.is_helper_move_allowed =
            path != lib_path && is_helper_move_allowed(&sources[path].items, &group.names);
    }
    let mut retained = BTreeMap::new();
    for (path, syntax) in sources {
        let group = groups.get(path);
        let mut references = References {
            module: module_path(path, lib_path),
            lib_name: lib_name.to_owned(),
            is_self_reference_checked: group.is_none_or(|group| !group.is_helper_move_allowed),
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
        if let Some(reason) = references.isolation_block_reason {
            retained.insert(path.clone(), reason);
        }
        if syntax
            .items
            .iter()
            .any(|item| matches!(item, Item::Macro(item) if item.ident.is_none()))
        {
            retained.insert(path.clone(), IsolationBlockReason::MacroGeneratedItem);
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
    retained: &mut BTreeMap<PathBuf, IsolationBlockReason>,
    references: &References,
    groups: &BTreeMap<PathBuf, HandlerModule<'_>>,
    own_path: Option<&Path>,
) {
    for (path, group) in groups {
        if own_path == Some(path.as_path()) {
            continue;
        }
        if references.paths.iter().any(|(reference, is_namespace)| {
            group.targets.iter().any(|target| {
                reference == target || (*is_namespace && target.starts_with(reference))
            })
        }) {
            retained.insert(
                path.clone(),
                IsolationBlockReason::CrossModuleHandlerReference,
            );
        }
    }
}

/// Allow helpers to move only when the file exposes no other shared items.
fn is_helper_move_allowed(items: &[Item], handlers: &BTreeSet<&str>) -> bool {
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
    /// When set, handlers in this file must stay in the shared library.
    isolation_block_reason: Option<IsolationBlockReason>,
    is_self_reference_checked: bool,
    external_prefixes: Vec<String>,
    private_macros: BTreeSet<String>,
    own_macros: BTreeSet<String>,
}

impl References {
    /// Macro bodies are opaque to the AST visitor, so inspect their tokens directly.
    /// Treat identifier paths as possible value references without expanding the macro.
    fn record_macro_paths(&mut self, mut cursor: Cursor<'_>) {
        while let Some((_, rest)) = cursor.token_tree() {
            if let Some((inner, _, _, rest)) = cursor.any_group() {
                // References can be nested inside (), [] or {}, as in dbg!({ worker() }).
                self.record_macro_paths(inner);
                cursor = rest;
            } else if let Some((ident, rest)) = cursor.ident() {
                let mut parts = vec![ident.to_string()];
                cursor = rest;
                // Keep other::worker as one path so worker is not resolved in this module.
                while let Some((first, rest)) = cursor.punct() {
                    let Some((second, rest)) = rest.punct() else {
                        break;
                    };
                    if first.as_char() != ':' || second.as_char() != ':' {
                        break;
                    }
                    let Some((ident, rest)) = rest.ident() else {
                        break;
                    };
                    parts.push(ident.to_string());
                    cursor = rest;
                }
                if parts.iter().any(|part| part == "from_worker") {
                    // Calls inside macros cannot be rewritten to from_name, so preserve
                    // the original worker type identity by keeping this file shared.
                    self.isolation_block_reason =
                        Some(IsolationBlockReason::WorkerReferenceInMacro);
                }
                self.record(parts, false, true);
            } else {
                // Text such as "worker" or "from_worker" inside a literal is not code.
                cursor = rest;
            }
        }
    }

    /// Resolve a path and flag references that require the original module.
    fn record(&mut self, parts: Vec<String>, is_namespace: bool, is_expression: bool) {
        if parts.is_empty() {
            return;
        }
        if !self.external_prefixes.is_empty() {
            if self.external_prefixes.contains(&parts[0]) {
                self.paths.push((parts[1..].to_vec(), is_namespace));
            }
            return;
        }
        let resolved = rust_paths::resolve(&parts, &self.module, &self.lib_name);
        if resolved == ["main"] || resolved == ["__kinetics_user_function"] {
            self.isolation_block_reason = Some(IsolationBlockReason::GeneratedNameConflict);
        }
        if self.is_self_reference_checked
            && is_expression
            && resolved.len() == self.module.len() + 1
            && resolved.starts_with(&self.module)
            && resolved
                .last()
                .is_some_and(|name| self.own_handlers.contains(name))
        {
            self.isolation_block_reason = Some(IsolationBlockReason::SharedCodeReference);
        }
        self.paths.push((resolved, is_namespace));
    }
}

impl<'ast> Visit<'ast> for References {
    /// Inspect opaque macro tokens and retain files that need their original context.
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if mac.path.segments.last().is_some_and(|segment| {
            let name = segment.ident.to_string();
            self.private_macros.contains(&name) && !self.own_macros.contains(&name)
        }) {
            self.isolation_block_reason = Some(IsolationBlockReason::PrivateMacro);
        }
        if mac.path.segments.last().is_some_and(|segment| {
            matches!(
                segment.ident.to_string().as_str(),
                "include" | "include_str" | "include_bytes"
            )
        }) {
            self.isolation_block_reason = Some(IsolationBlockReason::RelativeInclude);
        }
        let tokens = TokenBuffer::new2(mac.tokens.clone());
        self.record_macro_paths(tokens.begin());
        syn::visit::visit_macro(self, mac);
    }

    /// Allow handler-to-handler calls within a file that will move together.
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        let is_self_reference_checked = self.is_self_reference_checked;
        if self.own_handlers.contains(&function.sig.ident.to_string()) {
            self.is_self_reference_checked = false;
        }
        syn::visit::visit_item_fn(self, function);
        self.is_self_reference_checked = is_self_reference_checked;
    }

    /// Keep unresolved from_worker calls in their original module to preserve type identity.
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if matches!(&*call.func, syn::Expr::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "from_worker"))
        {
            // Unknown aliases/expressions keep the original function type identity.
            self.isolation_block_reason = Some(IsolationBlockReason::UnresolvedWorkerCall);
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
