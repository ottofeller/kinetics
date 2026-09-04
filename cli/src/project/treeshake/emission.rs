use super::database::AnalysisDatabase;
use super::reachability::{
    is_local_module, FullRetentionReason, ReachabilityResult, RetentionMode,
};
use eyre::{Context, ContextCompat};
use ra_ap_base_db::FileId;
use ra_ap_hir::{attach_db, db::HirDatabase, Function, Module, Semantics};
use ra_ap_syntax::ast::{HasModuleItem, HasName, HasVisibility};
use ra_ap_syntax::{ast, AstNode};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// How a source-tree file should be emitted into a generated function crate.
#[derive(Debug)]
pub(crate) enum FileEmission {
    /// Copy the source bytes without interpreting the file as a Rust module.
    Exact,
    /// Omit an unretained Rust module file.
    Skip,
    /// Write a retained Rust module after applying semantic edits.
    Rendered(String),
}

/// A semantic edit to apply to a Rust module source.
#[derive(Debug)]
enum SourceEdit {
    /// Replace a byte range with new source text.
    Replacement { range: Range<usize>, text: String },
    /// Remove a byte range and its trailing horizontal space and newline.
    Removal { range: Range<usize> },
}

impl SourceEdit {
    /// Resolves an edit to the exact range and replacement used during emission.
    fn resolve<'edit>(&'edit self, content: &str) -> (Range<usize>, &'edit str) {
        match self {
            Self::Replacement { range, text } => (range.clone(), text),
            Self::Removal { range } => (removal_range(content, range.clone()), ""),
        }
    }
}

/// A Rust file recognized as a package-local module by rust-analyzer.
#[derive(Debug)]
struct ModuleFile {
    /// rust-analyzer identifier used for semantic queries.
    id: FileId,
    /// Physical source path used by the copy phase.
    path: PathBuf,
}

/// Describes which module files and source edits a generated function crate needs.
#[derive(Debug)]
pub(crate) struct EmissionPlan {
    /// Top-level module that the generated library root must export.
    top_level_module: Option<String>,
    /// Function path relative to the generated library crate.
    function_path: String,
    /// Physical Rust files that rust-analyzer recognizes as package modules under `src`.
    module_paths: HashSet<PathBuf>,
    /// Physical module paths retained for this function.
    retained_paths: HashSet<PathBuf>,
    /// Semantic source edits keyed by physical path, without rust-analyzer types.
    source_edits: HashMap<PathBuf, Vec<SourceEdit>>,
    /// Whether the traversal stayed selective and, if not, its first fallback reason.
    mode: RetentionMode,
}

impl EmissionPlan {
    /// Builds an emission boundary from semantic reachability and module declarations.
    pub(super) fn build(
        database: &AnalysisDatabase,
        function: Function,
        function_name: &str,
        mut reachability: ReachabilityResult,
    ) -> Self {
        let semantics = Semantics::new_dyn(database.host.raw_database());
        let module_files = collect_module_files(database, &semantics);
        let selected_modules = function
            .module(semantics.db)
            .path_to_root(semantics.db)
            .into_iter()
            .collect::<HashSet<_>>();
        let (top_level_module, function_path) =
            function_location(database, function, function_name);

        let mut source_edits = plan_visibility_edits(
            &semantics,
            &module_files,
            &selected_modules,
            &mut reachability.mode,
        );
        merge_edits(
            &mut source_edits,
            plan_orphan_edits(
                database,
                &semantics,
                &module_files,
                &reachability.retained_files,
                &mut reachability.mode,
            ),
        );

        let module_paths = module_files
            .iter()
            .map(|module_file| module_file.path.clone())
            .collect();
        let retained_paths = retained_module_paths(database, &module_files, &reachability);

        Self {
            top_level_module,
            function_path,
            module_paths,
            retained_paths,
            source_edits,
            mode: reachability.mode,
        }
    }

    /// Decides whether to copy, omit, or rewrite one source-tree file.
    pub(crate) fn file_emission(&self, path: &Path) -> eyre::Result<FileEmission> {
        if !self.module_paths.contains(path) {
            return Ok(FileEmission::Exact);
        }
        if !self.retained_paths.contains(path) {
            return Ok(FileEmission::Skip);
        }

        Ok(FileEmission::Rendered(self.emit_file_content(path)?))
    }

    /// Creates the wrapper import for the selected function and generated library crate.
    pub(crate) fn function_import(&self, crate_name: &str) -> String {
        let crate_name = crate_name.replace('-', "_");
        format!("use {crate_name}::{};", self.function_path)
    }

    /// Reads a retained module file and applies its planned semantic edits.
    fn emit_file_content(&self, path: &Path) -> eyre::Result<String> {
        let mut content = fs::read_to_string(path)
            .wrap_err_with(|| format!("Failed to read Rust source {path:?}"))?;
        let Some(edits) = self.source_edits.get(path) else {
            return Ok(content);
        };

        apply_source_edits(&mut content, path, edits)?;
        Ok(content)
    }

    /// Emits `lib.rs` while ensuring the selected top-level module is publicly exported once.
    pub(crate) fn emit_lib(&self, path: &Path) -> eyre::Result<String> {
        let Some(module_name) = self.top_level_module.as_deref() else {
            return self.emit_file_content(path);
        };
        if !path.exists() {
            return Ok(format!("pub mod {module_name};\n"));
        }

        let mut content = self.emit_file_content(path)?;
        let parse = ra_ap_syntax::SourceFile::parse(&content, ra_ap_syntax::Edition::CURRENT);
        let source_file = parse.tree();
        let normalized_module_name = module_name.trim_start_matches("r#");
        let declaration = source_file.items().find_map(|item| {
            let module = ast::Module::cast(item.syntax().clone())?;
            (module.name()?.text().trim_start_matches("r#") == normalized_module_name)
                .then_some(module)
        });

        let Some(declaration) = declaration else {
            return Ok(format!("pub mod {module_name};\n{content}"));
        };
        let edit = if let Some(visibility) = declaration.visibility() {
            if visibility.syntax().text() == "pub" {
                return Ok(content);
            }
            let range = visibility.syntax().text_range();
            SourceEdit::Replacement {
                range: usize::from(range.start())..usize::from(range.end()),
                text: "pub".to_owned(),
            }
        } else {
            let mod_token = declaration.mod_token().wrap_err_with(|| {
                format!("Module {module_name:?} in {path:?} has no `mod` token")
            })?;
            let offset = usize::from(mod_token.text_range().start());
            SourceEdit::Replacement {
                range: offset..offset,
                text: "pub ".to_owned(),
            }
        };
        apply_source_edits(&mut content, path, &[edit])?;
        Ok(content)
    }

    /// Logs the final retention mode and source count for one selected function.
    pub(super) fn log_summary(&self, function_name: &str) {
        match &self.mode {
            RetentionMode::Selective => log::debug!(
                "TreeShaker {function_name}: selective, {} retained module files",
                self.retained_paths.len()
            ),
            RetentionMode::Full { reason } => log::debug!(
                "TreeShaker {function_name}: full, {} retained module files; reason: {reason}",
                self.retained_paths.len()
            ),
        }
    }
}

/// Collects package-local module files under `src` in deterministic path order.
fn collect_module_files(
    database: &AnalysisDatabase,
    semantics: &Semantics<'_, dyn HirDatabase>,
) -> Vec<ModuleFile> {
    let src_root = database.package_root.join("src");
    let mut module_files = Vec::new();

    attach_db(semantics.db, || {
        for (id, path) in database.source_index.files() {
            if path.starts_with(&src_root)
                && semantics
                    .file_to_module_defs(id)
                    .any(|module| is_local_module(module, semantics.db))
            {
                module_files.push(ModuleFile {
                    id,
                    path: path.to_path_buf(),
                });
            }
        }
    });
    module_files.sort_by(|left, right| left.path.cmp(&right.path));
    module_files
}

/// Plans visibility changes along the selected function's module chain.
fn plan_visibility_edits(
    semantics: &Semantics<'_, dyn HirDatabase>,
    module_files: &[ModuleFile],
    selected_modules: &HashSet<Module>,
    mode: &mut RetentionMode,
) -> HashMap<PathBuf, Vec<SourceEdit>> {
    let mut edits: HashMap<PathBuf, Vec<SourceEdit>> = HashMap::new();

    attach_db(semantics.db, || {
        for module_file in module_files {
            let source_file = semantics.parse_guess_edition(module_file.id);
            for module_declaration in source_file
                .syntax()
                .descendants()
                .filter_map(ast::Module::cast)
            {
                let Some(module) = semantics.to_module_def(&module_declaration) else {
                    continue;
                };
                if !selected_modules.contains(&module) {
                    continue;
                }
                match public_visibility_edit(&module_declaration, &module_file.path) {
                    Ok(Some(edit)) => edits
                        .entry(module_file.path.clone())
                        .or_default()
                        .push(edit),
                    Ok(None) => {}
                    Err(reason) => mode.require_full(reason),
                }
            }
        }
    });

    edits
}

/// Plans removals of declarations whose out-of-line modules are not retained.
fn plan_orphan_edits(
    database: &AnalysisDatabase,
    semantics: &Semantics<'_, dyn HirDatabase>,
    module_files: &[ModuleFile],
    retained_files: &HashSet<FileId>,
    mode: &mut RetentionMode,
) -> HashMap<PathBuf, Vec<SourceEdit>> {
    if !matches!(mode, RetentionMode::Selective) {
        return HashMap::new();
    }

    let mut edits = HashMap::new();
    attach_db(semantics.db, || {
        for module_file in module_files {
            if !retained_files.contains(&module_file.id) {
                continue;
            }
            let source_file = semantics.parse_guess_edition(module_file.id);
            let mut file_edits = Vec::new();

            for module_declaration in source_file
                .syntax()
                .descendants()
                .filter_map(ast::Module::cast)
            {
                let Some(module) = semantics.to_module_def(&module_declaration) else {
                    if module_declaration.item_list().is_some() {
                        continue;
                    }
                    mode.require_full(FullRetentionReason::UnresolvedModuleDeclaration {
                        path: module_file.path.clone(),
                    });
                    break;
                };

                if module_declaration.item_list().is_some() {
                    continue;
                }
                let Some(target_file_id) = module
                    .as_source_file_id(semantics.db)
                    .map(|file_id| file_id.file_id(semantics.db))
                else {
                    mode.require_full(FullRetentionReason::UnresolvedModuleDeclaration {
                        path: module_file.path.clone(),
                    });
                    break;
                };
                if !database.source_index.contains(target_file_id) {
                    mode.require_full(FullRetentionReason::UnregisteredModuleSource {
                        path: module_file.path.clone(),
                    });
                    break;
                }
                if !retained_files.contains(&target_file_id) {
                    let text_range = module_declaration.syntax().text_range();
                    file_edits.push(SourceEdit::Removal {
                        range: usize::from(text_range.start())..usize::from(text_range.end()),
                    });
                }
            }

            if !file_edits.is_empty() {
                edits.insert(module_file.path.clone(), file_edits);
            }
            if !matches!(mode, RetentionMode::Selective) {
                break;
            }
        }
    });

    if matches!(mode, RetentionMode::Full { .. }) {
        edits.clear();
    }
    edits
}

/// Computes the module export and wrapper import paths for the selected function.
fn function_location(
    database: &AnalysisDatabase,
    function: Function,
    function_name: &str,
) -> (Option<String>, String) {
    let hir_database = database.host.raw_database();
    let mut modules = function.module(hir_database).path_to_root(hir_database);
    modules.reverse();
    let mut path = modules
        .into_iter()
        .filter_map(|module| {
            let name = module.name(hir_database)?;
            let edition = module.krate().edition(hir_database);
            let displayed_name = name.display(hir_database, edition).to_string();
            Some(displayed_name)
        })
        .collect::<Vec<_>>();

    if database.crate_root_path == database.package_root.join("src/main.rs") {
        path.insert(0, "main".to_owned());
    }
    let top_level_module = path.first().cloned();
    path.push(function_name.to_owned());
    (top_level_module, path.join("::"))
}

/// Converts reachability file identifiers to the final physical retention set.
fn retained_module_paths(
    database: &AnalysisDatabase,
    module_files: &[ModuleFile],
    reachability: &ReachabilityResult,
) -> HashSet<PathBuf> {
    match &reachability.mode {
        RetentionMode::Selective => reachability
            .retained_files
            .iter()
            .filter_map(|file_id| database.source_index.path(*file_id).map(PathBuf::from))
            .collect(),
        RetentionMode::Full { .. } => module_files
            .iter()
            .map(|module_file| module_file.path.clone())
            .collect(),
    }
}

/// Adds one edit map to another while preserving edit discovery order per file.
fn merge_edits(
    destination: &mut HashMap<PathBuf, Vec<SourceEdit>>,
    source: HashMap<PathBuf, Vec<SourceEdit>>,
) {
    for (path, edits) in source {
        destination.entry(path).or_default().extend(edits);
    }
}

/// Creates an AST-derived edit that exports a selected module without moving its attributes.
fn public_visibility_edit(
    module: &ast::Module,
    path: &Path,
) -> Result<Option<SourceEdit>, FullRetentionReason> {
    if let Some(visibility) = module.visibility() {
        if visibility.syntax().text() == "pub" {
            return Ok(None);
        }
        let range = visibility.syntax().text_range();
        return Ok(Some(SourceEdit::Replacement {
            range: usize::from(range.start())..usize::from(range.end()),
            text: "pub".to_owned(),
        }));
    }

    let mod_token =
        module
            .mod_token()
            .ok_or_else(|| FullRetentionReason::UneditableSelectedModule {
                path: path.to_path_buf(),
            })?;
    let offset = usize::from(mod_token.text_range().start());
    Ok(Some(SourceEdit::Replacement {
        range: offset..offset,
        text: "pub ".to_owned(),
    }))
}

/// Applies source edits from the end of the file so earlier byte ranges stay valid.
fn apply_source_edits(content: &mut String, path: &Path, edits: &[SourceEdit]) -> eyre::Result<()> {
    let mut resolved_edits = edits
        .iter()
        .map(|edit| edit.resolve(content))
        .collect::<Vec<_>>();
    resolved_edits.sort_by_key(|edit| Reverse(edit.0.start));

    for (range, replacement) in resolved_edits {
        if range.end > content.len() {
            return Err(eyre::eyre!(
                "Semantic source edit is outside the current content of {path:?}"
            ));
        }
        content.replace_range(range, replacement);
    }
    Ok(())
}

/// Extends a removed declaration through its trailing horizontal space and one newline.
fn removal_range(content: &str, mut range: Range<usize>) -> Range<usize> {
    while range.end < content.len() && matches!(content.as_bytes()[range.end], b' ' | b'\t') {
        range.end += 1;
    }
    if content.as_bytes().get(range.end) == Some(&b'\r') {
        range.end += 1;
    }
    if content.as_bytes().get(range.end) == Some(&b'\n') {
        range.end += 1;
    }
    range
}
