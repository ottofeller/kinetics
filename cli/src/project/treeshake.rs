use kinetics_parser::ParsedFunction;
use ra_ap_base_db::{
    CrateGraphBuilder, CrateOrigin, CrateWorkspaceData, Env, FileId, FileSet, SourceRoot, VfsPath,
};
use ra_ap_hir::ChangeWithProcMacros;
use ra_ap_hir::{
    attach_db, db::HirDatabase, Adt, Crate, Function, Module, ModuleDef, PathResolution, ScopeDef,
    Semantics,
};
use ra_ap_ide::{AnalysisHost, Edition, RootDatabase};
use ra_ap_paths::AbsPathBuf;
use ra_ap_syntax::{ast, AstNode};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use triomphe::Arc;
use walkdir::WalkDir;

#[derive(Debug)]
pub struct TreeShaker {
    source_root: Option<SourceRoot>,
    file_set: FileSet,
    id_to_path: HashMap<FileId, PathBuf>,
    host: Option<AnalysisHost>,
}

impl TreeShaker {
    pub fn new() -> Self {
        Self {
            source_root: None,
            file_set: FileSet::default(),
            id_to_path: HashMap::new(),
            host: None,
        }
    }

    /// Initialize the dependency graph by
    /// - scanning the source directory;
    /// - apply all files as changes to register their content within the graph.
    pub fn initialize(&mut self, src_dir: &Path) -> eyre::Result<()> {
        let mut file_id_counter = 0u32;
        let mut file_set = FileSet::default();
        let mut id_to_path = HashMap::new();
        let mut file_contents = HashMap::new();

        log::debug!("walk with TreeShaker: {src_dir:?}");
        // 1. Recursively scan for .rs files and collect contents
        for entry in WalkDir::new(src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "rs"))
        {
            let path = entry.path();
            if let Ok(content) = fs::read_to_string(path) {
                // let relative_path = path.strip_prefix(src_dir).unwrap_or(path);
                let vfs_path = VfsPath::new_virtual_path(path.to_string_lossy().into_owned());

                let fid = FileId::from_raw(file_id_counter);
                file_set.insert(fid, vfs_path);
                id_to_path.insert(fid, path.to_path_buf());
                file_contents.insert(fid, content);

                file_id_counter += 1;
            }
        }

        // 2. Build SourceRoot and CrateGraph
        let source_root = SourceRoot::new_local(file_set.clone());
        let mut crate_graph = CrateGraphBuilder::default();

        // Find the root of the package (src/lib.rs or src/main.rs)
        let root_file_id = id_to_path.iter().find_map(|(&id, path)| {
            if path.ends_with("src/lib.rs") || path.ends_with("src/main.rs") {
                Some(id)
            } else {
                None
            }
        });

        if let Some(fid) = root_file_id {
            log::debug!("add root: {fid:?}");
            let proc_macro_cwd =
                Arc::new(AbsPathBuf::assert_utf8(std::env::current_dir().unwrap()));
            crate_graph.add_crate_root(
                fid,
                Edition::CURRENT,
                None,               // display_name
                None,               // version
                Default::default(), // cfg_options
                None,               // potential_cfg_options
                Env::default(),
                CrateOrigin::Local {
                    repo: None,
                    name: None,
                },
                false, // is_proc_macro
                proc_macro_cwd,
                Arc::new(CrateWorkspaceData {
                    target: Err("no layout".into()),
                    toolchain: None,
                }),
            );
        }

        log::debug!("create AnalysisHost");
        // 3. Create AnalysisHost and apply changes
        let mut host = AnalysisHost::new(None);
        let mut change = ChangeWithProcMacros::default();
        change.set_roots(vec![source_root.clone()]);
        for (fid, content) in &file_contents {
            change.change_file(*fid, Some(content.clone()));
        }
        change.set_crate_graph(crate_graph);

        log::debug!("apply_change to AnalysisHost");
        host.apply_change(change);
        log::debug!("change applied");
        self.host = Some(host);
        self.source_root = Some(source_root);
        self.file_set = file_set;
        self.id_to_path = id_to_path;

        Ok(())
    }

    /// Consume the TreeShaker and build a DependencyGraph from the given entry point.
    pub fn into_dependency_graph(
        mut self,
        parsed_function: &ParsedFunction,
    ) -> eyre::Result<DependencyGraph> {
        let host = self
            .host
            .take()
            .ok_or_else(|| eyre::eyre!("TreeShaker not initialized"))?;
        let id_to_path = std::mem::take(&mut self.id_to_path);
        log::debug!("new DependencyGraph");
        let mut graph = DependencyGraph::new(host, id_to_path);
        log::debug!("build DependencyGraph");
        graph.build_from(parsed_function)?;
        Ok(graph)
    }
}

/// Wraps the `rust-analyzer` `Analysis` snapshot and provides recursive
/// reachability analysis starting from a `ParsedFunction`.
#[derive(Debug)]
pub struct DependencyGraph {
    host: AnalysisHost,
    id_to_path: HashMap<FileId, PathBuf>,
    /// Set of reached `ModuleDef`s discovered during traversal.
    reached: HashSet<ModuleDef>,
}

impl DependencyGraph {
    pub fn new(host: AnalysisHost, id_to_path: HashMap<FileId, PathBuf>) -> Self {
        Self {
            host,
            id_to_path,
            reached: HashSet::new(),
        }
    }

    fn db(&self) -> &dyn HirDatabase {
        self.host.raw_database()
    }

    fn semantics(&self) -> Semantics<'_, dyn HirDatabase> {
        Semantics::new_dyn(self.host.raw_database())
    }

    /// Locate the target function in the HIR and recursively traverse all
    /// referenced items.
    pub fn build_from(&mut self, parsed_function: &ParsedFunction) -> eyre::Result<()> {
        let func = self.find_function(parsed_function)?;
        // A single `Semantics` instance must be used for the entire traversal:
        // its `root_to_file_cache` is populated by `source` calls and consumed
        // by `resolve_path` calls, and the two must share the same instance.
        let semantics = Semantics::new_dyn(self.host.raw_database());
        // `attach_db` registers the database in a thread-local so that the
        // rust-analyzer "next solver" interner (used during type inference,
        // which `resolve_path` triggers) can reach it. Without it, the
        // interner panics with `Try to use attached db, but not db is attached`.
        attach_db(semantics.db, || {
            traverse_function(&mut self.reached, func, &semantics)
        });
        Ok(())
    }

    fn find_function(&self, parsed_function: &ParsedFunction) -> eyre::Result<Function> {
        let db = self.db();

        let krate = Crate::all(db)
            .into_iter()
            .find(|c| matches!(c.origin(db), ra_ap_base_db::CrateOrigin::Local { .. }))
            .ok_or_else(|| eyre::eyre!("No local crate found in the analysis database"))?;
        let module = self.resolve_module(krate, &parsed_function.relative_path)?;

        // Look up the function name in the module's scope.
        let func_name = &parsed_function.rust_function_name;
        for (name, scope_def) in module.scope(db, None) {
            if name.as_str() == *func_name {
                if let ScopeDef::ModuleDef(ModuleDef::Function(f)) = scope_def {
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

        // If the stem is empty, "lib", or "main", we're at the crate root.
        if stem.is_empty() || stem == "lib" || stem == "main" {
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

        let mut module = root;
        for segment in &segments {
            let mut found = false;
            for child in module.children(db) {
                if let Some(name) = child.name(db) {
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

    /// Produce a `PrunedGraph` from the current set of reached items.
    ///
    /// If a file contains any reached top-level item, the entire file is retained.
    /// Files that contain no reached items are dropped entirely.
    pub fn prune(&self) -> PrunedGraph {
        let db = self.db();
        let mut file_to_items: HashMap<PathBuf, HashSet<String>> = HashMap::new();

        // Find the local crate.
        let Some(krate) = Crate::all(db)
            .into_iter()
            .find(|c| matches!(c.origin(db), CrateOrigin::Local { .. }))
        else {
            return PrunedGraph { file_to_items };
        };

        // Walk all modules in the crate.
        let mut modules_to_visit: Vec<Module> = vec![krate.root_module()];
        while let Some(module) = modules_to_visit.pop() {
            // Schedule child modules for visiting.
            for child in module.children(db) {
                modules_to_visit.push(child);
            }

            // Determine the file this module lives in.
            let file_id = match module_file_id(module, self.host.raw_database()) {
                Some(fid) => fid,
                None => continue,
            };
            let Some(path) = self.id_to_path.get(&file_id) else {
                continue;
            };

            // Check each declaration in this module.
            for decl in module.declarations(db) {
                if self.reached.contains(&decl) {
                    let name = decl
                        .name(db)
                        .map(|n| n.as_str().to_owned())
                        .unwrap_or_default();
                    file_to_items.entry(path.clone()).or_default().insert(name);
                }
            }
        }

        PrunedGraph { file_to_items }
    }
}

/// Mark a `Function` as reached and traverse its body.
fn traverse_function(
    reached: &mut HashSet<ModuleDef>,
    func: Function,
    semantics: &Semantics<'_, dyn HirDatabase>,
) {
    let def = ModuleDef::from(func);
    if !reached.insert(def) {
        return;
    }

    // `Semantics::source` (unlike `HasSource::source`) caches the parsed
    // file's root node in the semantics' `root_to_file_cache`, which is a
    // precondition for `resolve_path` to work on descendants of that node.
    if let Some(in_file) = semantics.source(func) {
        traverse_syntax_node(reached, in_file.value.syntax(), semantics);
    }
}

/// Walk a syntax node, resolving every `ast::Path`
/// and recursing into any referenced `ModuleDef`.
fn traverse_syntax_node(
    reached: &mut HashSet<ModuleDef>,
    node: &ra_ap_syntax::SyntaxNode,
    semantics: &Semantics<'_, dyn HirDatabase>,
) {
    for descendant in node.descendants() {
        let Some(path_node) = ast::Path::cast(descendant) else {
            continue;
        };
        if let Some(PathResolution::Def(module_def)) = semantics.resolve_path(&path_node) {
            traverse_module_def(reached, module_def, semantics);
        }
    }
}

/// Mark a `ModuleDef` as reached and, if it has a body or contains
/// further references, recurse into its source.
fn traverse_module_def(
    reached: &mut HashSet<ModuleDef>,
    module_def: ModuleDef,
    semantics: &Semantics<'_, dyn HirDatabase>,
) {
    if !reached.insert(module_def) {
        return;
    }

    match module_def {
        ModuleDef::Function(func) => {
            traverse_function(reached, func, semantics);
        }
        ModuleDef::Const(c) => {
            if let Some(in_file) = semantics.source(c) {
                traverse_syntax_node(reached, in_file.value.syntax(), semantics);
            }
        }
        ModuleDef::Static(s) => {
            if let Some(in_file) = semantics.source(s) {
                traverse_syntax_node(reached, in_file.value.syntax(), semantics);
            }
        }
        ModuleDef::Module(m) => {
            // Mark all declarations in this module as reached so that
            // the module itself and everything inside is kept.
            let decls: Vec<ModuleDef> = m.declarations(semantics.db);
            for decl in decls {
                traverse_module_def(reached, decl, semantics);
            }
        }
        ModuleDef::Adt(adt) => {
            let source = match adt {
                Adt::Struct(s) => semantics.source(s).map(|f| f.value.syntax().clone()),
                Adt::Enum(e) => semantics.source(e).map(|f| f.value.syntax().clone()),
                Adt::Union(u) => semantics.source(u).map(|f| f.value.syntax().clone()),
            };
            if let Some(syn) = source {
                traverse_syntax_node(reached, &syn, semantics);
            }
        }
        ModuleDef::Trait(t) => {
            if let Some(in_file) = semantics.source(t) {
                traverse_syntax_node(reached, in_file.value.syntax(), semantics);
            }
        }
        ModuleDef::TypeAlias(alias) => {
            if let Some(in_file) = semantics.source(alias) {
                traverse_syntax_node(reached, in_file.value.syntax(), semantics);
            }
        }
        // Macros: keep the entire definition verbatim (conservative).
        ModuleDef::Macro(_) => {}
        // Enum variants are handled via their parent enum.
        ModuleDef::Variant(_) => {}
        // Builtin types (u8, String, etc.) have no definition to traverse.
        ModuleDef::BuiltinType(_) => {}
    }
}

/// For file-backed modules returns the file directly.
/// For inline modules walks up to the nearest file-backed ancestor.
fn module_file_id(module: Module, db: &RootDatabase) -> Option<FileId> {
    let mut current = module;
    loop {
        if let Some(efid) = current.as_source_file_id(db) {
            return Some(efid.file_id(db));
        }
        // Inline module – try the parent.
        current = current.parent(db)?;
    }
}

/// Output of the shake, containing a mapping of file paths to a set of
/// retained top-level item names.
#[derive(Debug)]
pub struct PrunedGraph {
    /// Maps file paths to a set of retained top-level item names.
    /// An empty set means the file was not reached;
    /// a non-empty set means the file should be kept.
    pub file_to_items: HashMap<PathBuf, HashSet<String>>,
}

impl PrunedGraph {
    pub fn should_keep(&self, path: &Path) -> bool {
        self.file_to_items.contains_key(path)
    }

    pub fn retained_items(&self, path: &Path) -> Option<&HashSet<String>> {
        self.file_to_items.get(path)
    }
}
