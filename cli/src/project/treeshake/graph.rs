use super::RetainedFiles;
use kinetics_parser::ParsedFunction;
use ra_ap_base_db::{CrateOrigin, FileId};
use ra_ap_hir::{
    attach_db, db::HirDatabase, Crate, Function, HirFileId, Macro, Module, ModuleDef,
    PathResolution, ScopeDef, Semantics,
};
use ra_ap_ide::AnalysisHost;
use ra_ap_syntax::{ast, AstNode};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// Tracks source files reachable from one function through `rust-analyzer` definitions and macros
#[derive(Debug)]
pub(crate) struct DependencyGraph<'a> {
    /// Provides the semantic database used to resolve definitions, modules, and macros
    host: &'a AnalysisHost,
    /// Maps registered local file IDs to their physical source paths
    id_to_path: &'a HashMap<FileId, PathBuf>,
    /// Contains local source files found to be required by the target function
    retained_files: HashSet<FileId>,
    /// Queues newly retained files that still need their syntax scanned
    pending: VecDeque<FileId>,
    /// Records definitions already visited during dependency traversal
    processed_definitions: HashSet<ModuleDef>,
    /// Records macro expansion files already scanned for further dependencies
    processed_expansions: HashSet<HirFileId>,
    /// Indicates that pruning must retain every registered file after uncertain resolution
    conservative_fallback: bool,
}

impl<'a> DependencyGraph<'a> {
    pub(super) fn new(host: &'a AnalysisHost, id_to_path: &'a HashMap<FileId, PathBuf>) -> Self {
        Self {
            host,
            id_to_path,
            retained_files: HashSet::new(),
            pending: VecDeque::new(),
            processed_definitions: HashSet::new(),
            processed_expansions: HashSet::new(),
            conservative_fallback: false,
        }
    }

    fn db(&self) -> &dyn HirDatabase {
        self.host.raw_database()
    }

    /// Locate the target function in the HIR and scan retained files to a fixed point.
    pub(super) fn build_from(&mut self, parsed_function: &ParsedFunction) -> eyre::Result<()> {
        log::debug!(
            "build from relative_path {:?} and rust_function_name {}",
            parsed_function.relative_path,
            parsed_function.rust_function_name
        );
        let func = self.find_function(parsed_function)?;
        let semantics = Semantics::new_dyn(self.host.raw_database());
        attach_db(semantics.db, || {
            self.processed_definitions.insert(func.into());
            self.retain_module_chain(func.module(semantics.db), &semantics);

            while !self.conservative_fallback {
                let Some(file_id) = self.pending.pop_front() else {
                    break;
                };
                self.scan_file(file_id, &semantics);
            }
        });
        Ok(())
    }

    fn scan_file(&mut self, file_id: FileId, semantics: &Semantics<'_, dyn HirDatabase>) {
        log::debug!("scan retained file {file_id:?}");
        let source_file = semantics.parse_guess_edition(file_id);
        self.scan_syntax_node(source_file.syntax(), semantics);
    }

    fn scan_syntax_node(
        &mut self,
        node: &ra_ap_syntax::SyntaxNode,
        semantics: &Semantics<'_, dyn HirDatabase>,
    ) {
        for path in node.descendants().filter_map(ast::Path::cast) {
            if self.conservative_fallback {
                return;
            }
            if let Some(PathResolution::Def(module_def)) = semantics.resolve_path(&path) {
                self.reach_definition(module_def, semantics);
            }
        }

        for method_call in node.descendants().filter_map(ast::MethodCallExpr::cast) {
            if self.conservative_fallback {
                return;
            }
            if let Some(function) = semantics.resolve_method_call(&method_call) {
                self.reach_definition(function.into(), semantics);
            }
        }

        for macro_call in node.descendants().filter_map(ast::MacroCall::cast) {
            if self.conservative_fallback {
                return;
            }
            self.scan_macro_call(macro_call, semantics);
        }
    }

    fn reach_definition(
        &mut self,
        module_def: ModuleDef,
        semantics: &Semantics<'_, dyn HirDatabase>,
    ) {
        if !self.processed_definitions.insert(module_def) {
            return;
        }

        if let ModuleDef::Macro(macro_def) = module_def {
            if is_local_module(macro_def.module(semantics.db), semantics.db) {
                self.retain_macro_definition(macro_def, semantics);
            }
            return;
        }

        let module = match module_def {
            ModuleDef::Module(module) => Some(module),
            _ => module_def.module(semantics.db),
        };
        let Some(module) = module else {
            return;
        };
        if !is_local_module(module, semantics.db) {
            return;
        }

        self.retain_module_chain(module, semantics);
    }

    fn scan_macro_call(
        &mut self,
        macro_call: ast::MacroCall,
        semantics: &Semantics<'_, dyn HirDatabase>,
    ) {
        let Some(macro_def) = semantics.resolve_macro_call(&macro_call) else {
            return;
        };
        if !is_local_module(macro_def.module(semantics.db), semantics.db) {
            return;
        }

        self.processed_definitions
            .insert(ModuleDef::Macro(macro_def));

        if !self.retain_macro_definition(macro_def, semantics) {
            return;
        }

        let Some(expansion) = semantics.expand_macro_call(&macro_call) else {
            self.fallback(format_args!(
                "local macro {:?} has no expansion",
                macro_def.name(semantics.db)
            ));
            return;
        };
        if self.processed_expansions.insert(expansion.file_id) {
            self.scan_syntax_node(&expansion.value, semantics);
        }
    }

    fn retain_macro_definition(
        &mut self,
        macro_def: Macro,
        semantics: &Semantics<'_, dyn HirDatabase>,
    ) -> bool {
        let Some(source) = semantics.source(macro_def) else {
            self.fallback(format_args!(
                "local macro {:?} has no source",
                macro_def.name(semantics.db)
            ));
            return false;
        };
        let source_file_id = source
            .file_id
            .original_file(semantics.db)
            .file_id(semantics.db);

        if !self.retain_file(source_file_id, "local macro definition") {
            return false;
        }

        let source_modules: Vec<Module> = semantics
            .file_to_module_defs(source_file_id)
            .filter(|module| is_local_module(*module, semantics.db))
            .collect();

        if source_modules.is_empty() {
            self.fallback(format_args!(
                "local macro source file {source_file_id:?} has no local module"
            ));
            return false;
        }
        for module in source_modules {
            self.retain_module_chain(module, semantics);
            if self.conservative_fallback {
                return false;
            }
        }

        true
    }

    fn retain_module_chain(
        &mut self,
        mut module: Module,
        semantics: &Semantics<'_, dyn HirDatabase>,
    ) {
        loop {
            let file_id = physical_module_file_id(module, semantics.db);
            if !self.retain_file(file_id, "module chain") {
                return;
            }
            let Some(parent) = module.parent(semantics.db) else {
                return;
            };
            module = parent;
        }
    }

    fn retain_file(&mut self, file_id: FileId, reason: &str) -> bool {
        if !self.id_to_path.contains_key(&file_id) {
            self.fallback(format_args!(
                "{reason} resolved to unregistered source file {file_id:?}"
            ));
            return false;
        }
        if self.retained_files.insert(file_id) {
            self.pending.push_back(file_id);
        }
        true
    }

    fn fallback(&mut self, reason: impl std::fmt::Display) {
        log::debug!("TreeShaker conservative fallback: {reason}");
        self.conservative_fallback = true;
    }

    fn find_function(&self, parsed_function: &ParsedFunction) -> eyre::Result<Function> {
        let db = self.db();
        let all_krates = Crate::all(db);
        log::debug!("try find function in {} crates", all_krates.len());

        // Find the local crate (the one we loaded from disk).
        let krate = all_krates
            .into_iter()
            .find(|c| matches!(c.origin(db), ra_ap_base_db::CrateOrigin::Local { .. }))
            .ok_or_else(|| eyre::eyre!("No local crate found in the analysis database"))?;

        log::debug!("found local crate {:?}", krate.origin(db));

        // Walk the module tree to find the module containing the function.
        let module = self.resolve_module(krate, &parsed_function.relative_path)?;

        // Look up the function name in the module's scope.
        let func_name = &parsed_function.rust_function_name;
        for (name, scope_def) in module.scope(db, None) {
            log::debug!(
                "  scope entry: name={:?}, def={:?}",
                name.as_str(),
                scope_def
            );
            if name.as_str() == *func_name {
                if let ScopeDef::ModuleDef(ModuleDef::Function(f)) = scope_def {
                    log::debug!("found function {:?}", f.name(db));
                    return Ok(f);
                }
            }
        }

        Err(eyre::eyre!(
            "Function '{}' not found in module '{:?}'",
            func_name,
            parsed_function.relative_path
        ))
    }

    /// Walk the module tree from the crate root, following path segments
    /// extracted from `relative_path` (e.g. `src/foo/bar.rs` → `[foo, bar]`).
    fn resolve_module(&self, krate: Crate, relative_path: &Path) -> eyre::Result<Module> {
        let db = self.db();
        let root = krate.root_module();

        let path_str = relative_path.to_string_lossy();
        let stem = path_str
            .strip_prefix("src/")
            .or_else(|| path_str.strip_prefix("src\\"))
            .map(|p| p.strip_suffix(".rs").unwrap_or(p))
            .unwrap_or("");

        log::debug!("resolve module for path {path_str}, stem={stem}");

        // If the stem is empty, "lib", or "main", we're at the crate root.
        if stem.is_empty() || stem == "lib" || stem == "main" {
            log::debug!("resolve root module");
            return Ok(root);
        }

        // For `mod.rs` files, the last component is "mod", meaning the module
        // name is the parent directory.
        let segments_source = if stem.ends_with("/mod") || stem.ends_with("\\mod") {
            // parent
            stem.trim_end_matches("/mod").trim_end_matches("\\mod")
        } else {
            stem
        };
        let segments: Vec<&str> = segments_source
            .split('/')
            .flat_map(|s| s.split('\\'))
            .filter(|s| !s.is_empty())
            .collect();

        log::debug!("resolve module via segments {segments:?}");

        let mut module = root;
        for segment in &segments {
            let mut found = false;
            for child in module.children(db) {
                if let Some(name) = child.name(db) {
                    log::debug!("  child name: {:?}", name.as_str());
                    if name.as_str() == *segment {
                        module = child;
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                return Err(eyre::eyre!(
                    "Module segment '{}' not found while resolving path '{:?}'",
                    segment,
                    relative_path
                ));
            }
        }

        Ok(module)
    }

    /// Produce `RetainedFiles` from the retained file paths.
    pub(crate) fn prune(&self) -> RetainedFiles {
        let retained_paths = if self.conservative_fallback {
            self.id_to_path.values().cloned().collect()
        } else {
            self.retained_files
                .iter()
                .filter_map(|file_id| self.id_to_path.get(file_id).cloned())
                .collect()
        };

        RetainedFiles::new(retained_paths)
    }
}

fn is_local_module(module: Module, db: &dyn HirDatabase) -> bool {
    matches!(module.krate().origin(db), CrateOrigin::Local { .. })
}

fn physical_module_file_id(module: Module, db: &dyn HirDatabase) -> FileId {
    module.as_source_file_id(db).map_or_else(
        || {
            module
                .definition_source_file_id(db)
                .original_file(db)
                .file_id(db)
        },
        |file_id| file_id.file_id(db),
    )
}
