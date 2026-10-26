use kinetics_parser::ParsedFunction;
use std::collections::BTreeMap;
use std::path::Path;
use syn::{Item, UseTree};

/// Map a source filename to a Rust module path; the library root has an empty path.
pub(super) fn module_path(path: &Path, lib_path: &Path) -> Vec<String> {
    if path == lib_path {
        return Vec::new();
    }
    let path = path.strip_prefix("src").unwrap_or(path).with_extension("");
    let mut parts: Vec<_> = path
        .iter()
        .map(|part| part.to_string_lossy().into_owned())
        .collect();
    if parts.last().is_some_and(|part| part == "mod") {
        parts.pop();
    }
    parts
}

/// Resolve crate, library, self, and super prefixes to a crate-relative path.
pub(super) fn resolve(parts: &[String], module: &[String], lib_name: &str) -> Vec<String> {
    let mut resolved = module.to_vec();
    let mut offset = 0;
    match parts.first().map(String::as_str) {
        Some("crate") => {
            resolved.clear();
            offset = 1;
        }
        Some(name) if name == lib_name => {
            resolved.clear();
            offset = 1;
        }
        Some("self") => offset = 1,
        Some("super") => {
            while parts.get(offset).is_some_and(|part| part == "super") {
                resolved.pop();
                offset += 1;
            }
        }
        _ => {}
    }
    resolved.extend_from_slice(&parts[offset..]);
    resolved
}

/// Visit imported paths with their local binding; globs have no single binding.
pub(super) fn visit_imports(tree: &UseTree, visit: &mut impl FnMut(Vec<String>, Option<String>)) {
    /// Expand grouped imports while carrying their shared path prefix.
    fn walk(
        tree: &UseTree,
        mut path: Vec<String>,
        visit: &mut impl FnMut(Vec<String>, Option<String>),
    ) {
        match tree {
            UseTree::Path(item) => {
                path.push(item.ident.to_string());
                walk(&item.tree, path, visit);
            }
            UseTree::Name(item) => {
                if item.ident != "self" {
                    path.push(item.ident.to_string());
                }
                let binding = path.last().cloned();
                visit(path, binding);
            }
            UseTree::Rename(item) => {
                if item.ident != "self" {
                    path.push(item.ident.to_string());
                }
                visit(path, Some(item.rename.to_string()));
            }
            UseTree::Glob(_) => visit(path, None),
            UseTree::Group(group) => {
                for item in &group.items {
                    walk(item, path.clone(), visit);
                }
            }
        }
    }
    walk(tree, Vec::new(), visit);
}

/// Map local import names, including aliases, to their declared paths; omit globs.
pub(super) fn imports(items: &[Item]) -> BTreeMap<String, Vec<String>> {
    let mut imports = BTreeMap::new();
    for item in items {
        if let Item::Use(item) = item {
            visit_imports(&item.tree, &mut |path, binding| {
                if let Some(binding) = binding {
                    imports.insert(binding, path);
                }
            });
        }
    }
    imports
}

/// Return a handler's crate-relative path, including its function name.
pub(super) fn function_path(function: &ParsedFunction, lib_path: &Path) -> Vec<String> {
    let mut path = module_path(&function.relative_path, lib_path);
    path.push(function.rust_function_name.clone());
    path
}
