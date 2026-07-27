use super::database::AnalysisDatabase;
use ra_ap_base_db::{CrateOrigin, FileId};
use ra_ap_hir::{
    attach_db, db::HirDatabase, Function, HirFileId, Macro, Module, ModuleDef, PathResolution,
    Semantics,
};
use ra_ap_syntax::{ast, match_ast, AstNode};
use std::collections::{HashSet, VecDeque};
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

/// Explains why semantic pruning could not safely remain selective.
#[derive(Clone, Debug)]
pub(super) enum FullRetentionReason {
    /// A local declarative macro did not produce an analyzable expansion.
    MissingMacroExpansion { macro_name: String },
    /// A local declarative macro did not have a physical source.
    MissingMacroSource { macro_name: String },
    /// A local macro source could not be associated with an owning module.
    MacroSourceWithoutLocalModule { path: PathBuf },
    /// A resolved local definition pointed outside the loaded source index.
    UnregisteredDefinitionSource,
    /// An out-of-line module declaration could not be resolved unambiguously.
    UnresolvedModuleDeclaration { path: PathBuf },
    /// A resolved out-of-line module pointed outside the loaded source index.
    UnregisteredModuleSource { path: PathBuf },
    /// A selected module declaration could not be made public safely.
    UneditableSelectedModule { path: PathBuf },
}

impl Display for FullRetentionReason {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingMacroExpansion { macro_name } => {
                write!(formatter, "local macro {macro_name:?} has no expansion")
            }
            Self::MissingMacroSource { macro_name } => {
                write!(formatter, "local macro {macro_name:?} has no source")
            }
            Self::MacroSourceWithoutLocalModule { path } => {
                write!(formatter, "local macro source {path:?} has no local module")
            }
            Self::UnregisteredDefinitionSource => {
                formatter.write_str("a local definition resolved outside the loaded source index")
            }
            Self::UnresolvedModuleDeclaration { path } => {
                write!(
                    formatter,
                    "an out-of-line module declaration in {path:?} is unresolved"
                )
            }
            Self::UnregisteredModuleSource { path } => {
                write!(
                    formatter,
                    "module declaration in {path:?} resolves outside the loaded source index"
                )
            }
            Self::UneditableSelectedModule { path } => {
                write!(
                    formatter,
                    "selected module declaration in {path:?} cannot be made public"
                )
            }
        }
    }
}

/// Describes whether a function can use selective source retention.
#[derive(Clone, Debug)]
pub(super) enum RetentionMode {
    /// Only semantically reachable physical source files are retained.
    Selective,
    /// Every registered source file is retained after an uncertain resolution.
    Full { reason: FullRetentionReason },
}

impl RetentionMode {
    /// Records only the first reason requiring full retention.
    fn require_full(&mut self, reason: FullRetentionReason) {
        if matches!(self, Self::Selective) {
            log::trace!("TreeShaker full retention: {reason}");
            *self = Self::Full { reason };
        }
    }

    /// Reports whether traversal may continue selectively.
    fn is_selective(&self) -> bool {
        matches!(self, Self::Selective)
    }
}

/// File-level reachability result used to create an emission plan.
#[derive(Debug)]
pub(super) struct ReachabilityResult {
    /// Physical analysis files reached by the fixed-point traversal.
    pub(super) retained_files: HashSet<FileId>,
    /// Whether traversal completed selectively or conservatively.
    pub(super) mode: RetentionMode,
}

/// Computes a file-level semantic fixed point for one resolved function.
pub(super) struct Reachability<'db> {
    /// Loaded local database and source index.
    database: &'db AnalysisDatabase,
    /// Files discovered but not yet scanned for further dependencies.
    pending_files: VecDeque<FileId>,
    /// Files already retained by semantic traversal.
    retained_files: HashSet<FileId>,
    /// Definitions whose owning module chain has already been retained.
    seen_definitions: HashSet<ModuleDef>,
    /// Macro expansion HIR files already scanned for further dependencies.
    seen_expansions: HashSet<HirFileId>,
    /// Current selective or full-retention terminal state.
    mode: RetentionMode,
}

impl<'db> Reachability<'db> {
    /// Creates an empty traversal over a loaded package.
    pub(super) fn new(database: &'db AnalysisDatabase) -> Self {
        Self {
            database,
            pending_files: VecDeque::new(),
            retained_files: HashSet::new(),
            seen_definitions: HashSet::new(),
            seen_expansions: HashSet::new(),
            mode: RetentionMode::Selective,
        }
    }

    /// Scans the resolved function's owning files until no new files are reached.
    pub(super) fn build(mut self, function: Function) -> ReachabilityResult {
        let semantics = Semantics::new_dyn(self.database.host.raw_database());

        attach_db(semantics.db, || {
            self.retain_definition(function.into(), &semantics);

            while self.mode.is_selective() {
                let Some(file_id) = self.pending_files.pop_front() else {
                    break;
                };
                log::trace!("scan retained source {file_id:?}");
                let source_file = semantics.parse_guess_edition(file_id);
                self.scan_syntax_node(source_file.syntax(), &semantics);
            }
        });

        ReachabilityResult {
            retained_files: self.retained_files,
            mode: self.mode,
        }
    }

    /// Scans one source or macro-expansion syntax subtree in a single AST pass.
    fn scan_syntax_node(
        &mut self,
        node: &ra_ap_syntax::SyntaxNode,
        semantics: &Semantics<'_, dyn HirDatabase>,
    ) {
        for syntax_node in node.descendants() {
            if !self.mode.is_selective() {
                return;
            }

            match_ast! {
                match syntax_node {
                    ast::Path(path) => {
                        if let Some(PathResolution::Def(definition)) = semantics.resolve_path(&path) {
                            self.retain_definition(definition, semantics);
                        }
                    },
                    ast::MethodCallExpr(method_call) => {
                        if let Some(function) = semantics.resolve_method_call(&method_call) {
                            self.retain_definition(function.into(), semantics);
                        }
                    },
                    ast::MacroCall(macro_call) => self.scan_macro_call(macro_call, semantics),
                    _ => {},
                }
            }
        }
    }

    /// Retains the physical module chain owning a local definition.
    fn retain_definition(
        &mut self,
        definition: ModuleDef,
        semantics: &Semantics<'_, dyn HirDatabase>,
    ) {
        if !self.seen_definitions.insert(definition) {
            return;
        }

        let module = match definition {
            ModuleDef::Macro(macro_definition) => {
                if is_local_module(macro_definition.module(semantics.db), semantics.db) {
                    self.retain_macro_definition(macro_definition, semantics);
                }
                return;
            }
            ModuleDef::Module(module) => module,
            definition => {
                let Some(module) = definition.module(semantics.db) else {
                    return;
                };
                module
            }
        };

        if is_local_module(module, semantics.db) {
            self.retain_module_chain(module, semantics);
        }
    }

    /// Retains a local macro definition and scans each expansion HIR file once.
    fn scan_macro_call(
        &mut self,
        macro_call: ast::MacroCall,
        semantics: &Semantics<'_, dyn HirDatabase>,
    ) {
        let Some(macro_definition) = semantics.resolve_macro_call(&macro_call) else {
            return;
        };
        if !is_local_module(macro_definition.module(semantics.db), semantics.db) {
            return;
        }

        self.retain_definition(ModuleDef::Macro(macro_definition), semantics);
        if !self.mode.is_selective() {
            return;
        }

        let Some(expansion) = semantics.expand_macro_call(&macro_call) else {
            self.mode
                .require_full(FullRetentionReason::MissingMacroExpansion {
                    macro_name: macro_definition.name(semantics.db).as_str().to_owned(),
                });
            return;
        };
        if self.seen_expansions.insert(expansion.file_id) {
            self.scan_syntax_node(&expansion.value, semantics);
        }
    }

    /// Retains a macro's physical definition source and owning module chains.
    fn retain_macro_definition(
        &mut self,
        macro_definition: Macro,
        semantics: &Semantics<'_, dyn HirDatabase>,
    ) {
        let Some(source) = semantics.source(macro_definition) else {
            self.mode
                .require_full(FullRetentionReason::MissingMacroSource {
                    macro_name: macro_definition.name(semantics.db).as_str().to_owned(),
                });
            return;
        };
        let source_file_id = source
            .file_id
            .original_file(semantics.db)
            .file_id(semantics.db);

        if !self.retain_file(source_file_id) {
            return;
        }

        let source_modules: Vec<_> = semantics
            .file_to_module_defs(source_file_id)
            .filter(|module| is_local_module(*module, semantics.db))
            .collect();
        if source_modules.is_empty() {
            let path = self
                .database
                .source_index
                .path(source_file_id)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("<unregistered>"));
            self.mode
                .require_full(FullRetentionReason::MacroSourceWithoutLocalModule { path });
            return;
        }

        for module in source_modules {
            self.retain_module_chain(module, semantics);
            if !self.mode.is_selective() {
                return;
            }
        }
    }

    /// Retains a module's physical source and every declaring ancestor.
    fn retain_module_chain(
        &mut self,
        mut module: Module,
        semantics: &Semantics<'_, dyn HirDatabase>,
    ) {
        loop {
            let file_id = physical_module_file_id(module, semantics.db);
            if !self.retain_file(file_id) {
                return;
            }
            let Some(parent) = module.parent(semantics.db) else {
                return;
            };
            module = parent;
        }
    }

    /// Retains and queues a registered physical source file.
    fn retain_file(&mut self, file_id: FileId) -> bool {
        if !self.database.source_index.contains(file_id) {
            self.mode
                .require_full(FullRetentionReason::UnregisteredDefinitionSource);
            return false;
        }
        if self.retained_files.insert(file_id) {
            self.pending_files.push_back(file_id);
        }
        true
    }
}

/// Reports whether a module belongs to the package-local crate.
pub(super) fn is_local_module(module: Module, database: &dyn HirDatabase) -> bool {
    matches!(module.krate().origin(database), CrateOrigin::Local { .. })
}

/// Maps inline and out-of-line modules to their physical source file.
pub(super) fn physical_module_file_id(module: Module, database: &dyn HirDatabase) -> FileId {
    module.as_source_file_id(database).map_or_else(
        || {
            module
                .definition_source_file_id(database)
                .original_file(database)
                .file_id(database)
        },
        |file_id| file_id.file_id(database),
    )
}
