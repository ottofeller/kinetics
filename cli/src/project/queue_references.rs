use super::paths;
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;
use syn::visit_mut::VisitMut;
use syn::{Expr, UseTree};

/// `from_worker` only reads the function's type name. Keep that metadata independent
/// of the compiled handler body when preparing the generated workspace.
pub(super) fn lower(
    file: &mut syn::File,
    module: &[String],
    workers: &BTreeSet<Vec<String>>,
    lib_name: &str,
) {
    let imports = paths::imports(&file.items);
    let mut bindings = LocalBindings::default();
    bindings.visit_file(file);
    let mut lowering = Lowering {
        module,
        workers,
        lib_name,
        imports,
        bindings: bindings.names,
        consumed: BTreeSet::new(),
    };
    lowering.visit_file_mut(file);
    if lowering.consumed.is_empty() {
        return;
    }
    let mut used = Bindings::default();
    used.visit_file(file);
    let removable: BTreeSet<_> = lowering.consumed.difference(&used.0).cloned().collect();
    file.items.retain_mut(|item| match item {
        syn::Item::Use(item) if matches!(item.vis, syn::Visibility::Inherited) => {
            retain_imports(&mut item.tree, &removable)
        }
        _ => true,
    });
}

fn retain_imports(tree: &mut UseTree, removed: &BTreeSet<String>) -> bool {
    match tree {
        UseTree::Path(item) => retain_imports(&mut item.tree, removed),
        UseTree::Name(item) => !removed.contains(&item.ident.to_string()),
        UseTree::Rename(item) => !removed.contains(&item.rename.to_string()),
        UseTree::Group(items) => {
            let mut retained = syn::punctuated::Punctuated::new();
            for mut item in std::mem::take(&mut items.items) {
                if retain_imports(&mut item, removed) {
                    retained.push(item);
                }
            }
            items.items = retained;
            !items.items.is_empty()
        }
        UseTree::Glob(_) => true,
    }
}

#[derive(Default)]
struct Bindings(BTreeSet<String>);

#[derive(Default)]
struct LocalBindings {
    names: BTreeSet<String>,
    function_depth: usize,
}

impl<'ast> Visit<'ast> for LocalBindings {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        if self.function_depth > 0 {
            let mut imports = BTreeMap::new();
            paths::visit_imports(&item.tree, &mut |path, binding| {
                if let Some(binding) = binding {
                    imports.insert(binding, path);
                }
            });
            self.names.extend(imports.into_keys());
        }
    }

    fn visit_pat_ident(&mut self, binding: &'ast syn::PatIdent) {
        self.names.insert(binding.ident.to_string());
        syn::visit::visit_pat_ident(self, binding);
    }

    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        if self.function_depth > 0 {
            self.names.insert(function.sig.ident.to_string());
        }
        self.function_depth += 1;
        syn::visit::visit_item_fn(self, function);
        self.function_depth -= 1;
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.names.insert(item.ident.to_string());
        syn::visit::visit_item_struct(self, item);
    }

    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        self.names.insert(item.ident.to_string());
        syn::visit::visit_item_type(self, item);
    }
}

impl<'ast> Visit<'ast> for Bindings {
    fn visit_pat_ident(&mut self, binding: &'ast syn::PatIdent) {
        self.0.insert(binding.ident.to_string());
        syn::visit::visit_pat_ident(self, binding);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if let Some(segment) = path.segments.first() {
            self.0.insert(segment.ident.to_string());
        }
        syn::visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        // A macro may use an imported function. Keep such imports conservatively.
        self.0.extend(
            mac.tokens
                .to_string()
                .split(|character: char| !character.is_alphanumeric() && character != '_')
                .filter(|part| !part.is_empty())
                .map(str::to_owned),
        );
    }
}

struct Lowering<'a> {
    module: &'a [String],
    workers: &'a BTreeSet<Vec<String>>,
    lib_name: &'a str,
    imports: BTreeMap<String, Vec<String>>,
    bindings: BTreeSet<String>,
    consumed: BTreeSet<String>,
}

impl Lowering<'_> {
    fn expanded(&self, path: &syn::Path) -> Option<Vec<String>> {
        let parts: Vec<_> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        let first = parts.first()?;
        if self.bindings.contains(first) {
            return None;
        }
        Some(match self.imports.get(first) {
            Some(import) => import.iter().chain(&parts[1..]).cloned().collect(),
            None => parts,
        })
    }

    fn worker_path(&self, path: &syn::Path) -> Option<Vec<String>> {
        // A local function and an imported attribute macro can both be called
        // `worker`: the argument here belongs to the value namespace.
        if let Some(name) = path.get_ident() {
            let name = name.to_string();
            let mut local = self.module.to_vec();
            local.push(name.clone());
            if !self.bindings.contains(&name) && self.workers.contains(&local) {
                return Some(local);
            }
        }
        let parts = self.expanded(path)?;
        let local = paths::resolve(&parts, self.module, self.lib_name);
        if self.workers.contains(&local) {
            Some(local)
        } else if self.workers.contains(&parts) {
            Some(parts)
        } else {
            None
        }
    }
}

impl VisitMut for Lowering<'_> {
    fn visit_item_mod_mut(&mut self, _module: &mut syn::ItemMod) {
        // Inline modules have their own import scope; leave those calls unchanged.
    }

    fn visit_expr_call_mut(&mut self, call: &mut syn::ExprCall) {
        syn::visit_mut::visit_expr_call_mut(self, call);
        let Expr::Path(client) = &*call.func else {
            return;
        };
        if call.args.len() != 1 || client.qself.is_some() {
            return;
        }
        let Some(client_path) = self.expanded(&client.path) else {
            return;
        };
        if client_path != ["kinetics_lib", "tools", "queue", "Client", "from_worker"] {
            return;
        }
        let Some(Expr::Path(worker)) = call.args.first() else {
            return;
        };
        if worker.qself.is_some() {
            return;
        }
        let Some(worker_path) = self.worker_path(&worker.path) else {
            return;
        };
        if let Some(first) = worker.path.segments.first() {
            let alias = first.ident.to_string();
            if self.imports.contains_key(&alias) {
                self.consumed.insert(alias);
            }
        }
        let Expr::Path(client) = &mut *call.func else {
            unreachable!()
        };
        let segment = client
            .path
            .segments
            .last_mut()
            .expect("Client path is not empty");
        segment.ident = syn::Ident::new("from_name", segment.ident.span());
        let lib_name = self.lib_name;
        let worker_path = worker_path.join("::");
        call.args.clear();
        call.args.push(syn::parse_quote!(#lib_name));
        call.args.push(syn::parse_quote!(#worker_path));
    }
}
