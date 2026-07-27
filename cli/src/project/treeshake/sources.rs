use super::database::AnalysisDatabase;
use super::reachability::{
    is_local_module, FullRetentionReason, ReachabilityResult, RetentionMode,
};
use eyre::{Context, ContextCompat};
use ra_ap_hir::{attach_db, Function, Semantics};
use ra_ap_syntax::ast::{HasModuleItem, HasName, HasVisibility};
use ra_ap_syntax::{ast, AstNode};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// A source-only replacement computed from semantic module resolution.
#[derive(Debug)]
struct SourceEdit {
    /// Byte range replaced in the original source text.
    range: Range<usize>,
    /// Replacement text, empty when removing an orphan module declaration.
    replacement: String,
}

/// Describes which module files and source edits a generated function crate needs.
#[derive(Debug)]
pub(crate) struct FunctionSources {
    /// Semantic module segments from the crate root to the selected function.
    module_path: Vec<String>,
    /// Physical Rust files that rust-analyzer recognizes as package modules under `src`.
    module_paths: HashSet<PathBuf>,
    /// Physical module paths retained for this function.
    retained_paths: HashSet<PathBuf>,
    /// Semantic source edits keyed by physical path, without rust-analyzer types.
    source_edits: HashMap<PathBuf, Vec<SourceEdit>>,
    /// Whether the traversal stayed selective and, if not, its first fallback reason.
    mode: RetentionMode,
}

impl FunctionSources {
    /// Builds an emission boundary from semantic reachability and module declarations.
    pub(super) fn build(
        database: &AnalysisDatabase,
        function: Function,
        module_path: Vec<String>,
        mut reachability: ReachabilityResult,
    ) -> Self {
        let semantics = Semantics::new_dyn(database.host.raw_database());
        let src_root = database.package_root.join("src");
        let mut module_files = Vec::new();
        let selected_modules = function
            .module(semantics.db)
            .path_to_root(semantics.db)
            .into_iter()
            .collect::<HashSet<_>>();

        attach_db(semantics.db, || {
            for (file_id, path) in database.source_index.files() {
                if path.starts_with(&src_root)
                    && semantics
                        .file_to_module_defs(file_id)
                        .any(|module| is_local_module(module, semantics.db))
                {
                    module_files.push((file_id, path.to_path_buf()));
                }
            }
        });
        module_files.sort_by(|left, right| left.1.cmp(&right.1));

        let module_paths = module_files
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<HashSet<_>>();
        let mut retained_paths = match &reachability.mode {
            RetentionMode::Selective => reachability
                .retained_files
                .iter()
                .filter_map(|file_id| database.source_index.path(*file_id).map(PathBuf::from))
                .collect(),
            RetentionMode::Full { .. } => module_paths.clone(),
        };
        let mut visibility_edits: HashMap<PathBuf, Vec<SourceEdit>> = HashMap::new();
        attach_db(semantics.db, || {
            for (file_id, path) in &module_files {
                let source_file = semantics.parse_guess_edition(*file_id);
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
                    match public_visibility_edit(&module_declaration, path) {
                        Ok(Some(edit)) => {
                            visibility_edits.entry(path.clone()).or_default().push(edit)
                        }
                        Ok(None) => {}
                        Err(reason) => require_full(&mut reachability.mode, reason),
                    }
                }
            }
        });
        let mut orphan_edits = HashMap::new();

        if matches!(reachability.mode, RetentionMode::Selective) {
            attach_db(semantics.db, || {
                for (file_id, path) in &module_files {
                    if !reachability.retained_files.contains(file_id) {
                        continue;
                    }
                    let source_file = semantics.parse_guess_edition(*file_id);
                    let mut edits = Vec::new();

                    for module_declaration in source_file
                        .syntax()
                        .descendants()
                        .filter_map(ast::Module::cast)
                    {
                        let Some(module) = semantics.to_module_def(&module_declaration) else {
                            if module_declaration.item_list().is_some() {
                                continue;
                            }
                            require_full(
                                &mut reachability.mode,
                                FullRetentionReason::UnresolvedModuleDeclaration {
                                    path: path.clone(),
                                },
                            );
                            break;
                        };

                        if module_declaration.item_list().is_some() {
                            continue;
                        }
                        let Some(target_file_id) = module
                            .as_source_file_id(semantics.db)
                            .map(|file_id| file_id.file_id(semantics.db))
                        else {
                            require_full(
                                &mut reachability.mode,
                                FullRetentionReason::UnresolvedModuleDeclaration {
                                    path: path.clone(),
                                },
                            );
                            break;
                        };
                        if !database.source_index.contains(target_file_id) {
                            require_full(
                                &mut reachability.mode,
                                FullRetentionReason::UnregisteredModuleSource {
                                    path: path.clone(),
                                },
                            );
                            break;
                        }
                        if !reachability.retained_files.contains(&target_file_id) {
                            let text_range = module_declaration.syntax().text_range();
                            edits.push(SourceEdit {
                                range: usize::from(text_range.start())
                                    ..usize::from(text_range.end()),
                                replacement: String::new(),
                            });
                        }
                    }

                    if !edits.is_empty() {
                        orphan_edits.insert(path.clone(), edits);
                    }
                    if !matches!(reachability.mode, RetentionMode::Selective) {
                        break;
                    }
                }
            });
        }

        if matches!(reachability.mode, RetentionMode::Full { .. }) {
            retained_paths = module_paths.clone();
            orphan_edits.clear();
        }
        for (path, edits) in orphan_edits {
            visibility_edits.entry(path).or_default().extend(edits);
        }

        Self {
            module_path,
            module_paths,
            retained_paths,
            source_edits: visibility_edits,
            mode: reachability.mode,
        }
    }

    /// Reports whether a physical Rust source participates in the package module graph.
    pub(crate) fn is_module_file(&self, path: &Path) -> bool {
        self.module_paths.contains(path)
    }

    /// Reports whether a module source must be emitted for the selected function.
    pub(crate) fn should_keep(&self, path: &Path) -> bool {
        self.retained_paths.contains(path)
    }

    /// Returns the semantic module path used by the generated wrapper import.
    pub(crate) fn module_path(&self) -> &[String] {
        &self.module_path
    }

    /// Returns the top-level semantic module exported by the generated library root.
    fn top_level_module(&self) -> Option<&str> {
        self.module_path.first().map(String::as_str)
    }

    /// Reads a source file and applies semantic orphan-module edits when needed.
    pub(crate) fn emit_file_content(&self, path: &Path) -> eyre::Result<String> {
        let mut content = fs::read_to_string(path)
            .wrap_err_with(|| format!("Failed to read Rust source {path:?}"))?;
        let Some(edits) = self.source_edits.get(path) else {
            return Ok(content);
        };

        let mut ranges = edits
            .iter()
            .map(|edit| SourceEdit {
                range: if edit.replacement.is_empty() {
                    removal_range(&content, edit.range.clone())
                } else {
                    edit.range.clone()
                },
                replacement: edit.replacement.clone(),
            })
            .collect::<Vec<_>>();
        ranges.sort_by(|left, right| right.range.start.cmp(&left.range.start));

        for edit in ranges {
            if edit.range.end > content.len() {
                return Err(eyre::eyre!(
                    "Semantic source edit is outside the current content of {path:?}"
                ));
            }
            content.replace_range(edit.range, &edit.replacement);
        }
        Ok(content)
    }

    /// Emits `lib.rs` while ensuring the selected top-level module is publicly exported once.
    pub(crate) fn emit_lib(&self, path: &Path) -> eyre::Result<String> {
        let Some(module_name) = self.top_level_module() else {
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
            SourceEdit {
                range: usize::from(range.start())..usize::from(range.end()),
                replacement: "pub".to_owned(),
            }
        } else {
            let mod_token = declaration.mod_token().wrap_err_with(|| {
                format!("Module {module_name:?} in {path:?} has no `mod` token")
            })?;
            let offset = usize::from(mod_token.text_range().start());
            SourceEdit {
                range: offset..offset,
                replacement: "pub ".to_owned(),
            }
        };
        content.replace_range(edit.range, &edit.replacement);
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
        return Ok(Some(SourceEdit {
            range: usize::from(range.start())..usize::from(range.end()),
            replacement: "pub".to_owned(),
        }));
    }

    let mod_token =
        module
            .mod_token()
            .ok_or_else(|| FullRetentionReason::UneditableSelectedModule {
                path: path.to_path_buf(),
            })?;
    let offset = usize::from(mod_token.text_range().start());
    Ok(Some(SourceEdit {
        range: offset..offset,
        replacement: "pub ".to_owned(),
    }))
}

/// Records the first emission-time ambiguity that requires full retention.
fn require_full(mode: &mut RetentionMode, reason: FullRetentionReason) {
    if matches!(mode, RetentionMode::Selective) {
        *mode = RetentionMode::Full { reason };
    }
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
