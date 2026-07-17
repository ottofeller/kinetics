use eyre::Context;
use ra_ap_syntax::ast::HasModuleItem;
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{ast, AstNode};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Represents the source files retained for emission into a generated function crate
#[derive(Debug)]
pub(crate) struct RetainedFiles {
    /// Contains the physical source paths that must be copied into the generated crate
    retained_paths: HashSet<PathBuf>,
}

impl RetainedFiles {
    pub(super) fn new(retained_paths: HashSet<PathBuf>) -> Self {
        Self { retained_paths }
    }

    pub(crate) fn should_keep(&self, path: &Path) -> bool {
        self.retained_paths.contains(path)
    }

    /// Read a retained source file and return its content
    /// with `mod <name>;` declarations stripped,
    /// when the target module file is not retained
    pub(crate) fn emit_file_content(&self, src_path: &Path) -> eyre::Result<String> {
        let content = fs::read_to_string(src_path)
            .wrap_err_with(|| format!("Failed to read {src_path:?}"))?;

        let parse = ra_ap_syntax::SourceFile::parse(&content, ra_ap_syntax::Edition::CURRENT);
        let source_file = parse.tree();

        // Collect (in document order) the text ranges of orphan module
        // declarations we want to remove.  We'll process them in reverse
        // order so that earlier ranges remain valid after each removal.
        let mut ranges_to_remove: Vec<std::ops::Range<usize>> = Vec::new();

        for item in source_file.items() {
            let syn = item.syntax().clone();
            if let Some(mod_decl) = ast::Module::cast(syn) {
                // Only consider `mod name;` declarations — those without a body.
                if mod_decl.item_list().is_some() {
                    continue;
                }
                if let Some(name) = mod_decl.name() {
                    let mod_name = name.text().to_string();
                    if !self.module_file_retained(src_path, &mod_name) {
                        // Include trailing whitespace/newline so we don't
                        // leave blank lines behind.
                        let decl_range = mod_decl.syntax().text_range();
                        let start = usize::from(decl_range.start());
                        let mut end = usize::from(decl_range.end());
                        // Consume the following newline (and any trailing whitespace).
                        while end < content.len()
                            && (content.as_bytes()[end] == b'\n'
                                || content.as_bytes()[end] == b'\r'
                                || content.as_bytes()[end] == b' ')
                        {
                            end += 1;
                        }
                        log::debug!(
                            "  stripping orphan mod '{mod_name}' at bytes {start}..{end} from {src_path:?}"
                        );
                        ranges_to_remove.push(start..end);
                    }
                }
            }
        }

        if ranges_to_remove.is_empty() {
            return Ok(content);
        }

        // Remove ranges in reverse order so indices stay valid.
        ranges_to_remove.sort_by(|a, b| b.start.cmp(&a.start));
        let mut result = content;
        for range in &ranges_to_remove {
            result.replace_range(range.clone(), "");
        }

        Ok(result)
    }

    /// Check if a module file for `mod_name` referenced from `parent_rs_path`
    /// is present among the retained files.
    fn module_file_retained(&self, parent_rs_path: &Path, mod_name: &str) -> bool {
        let parent_dir = parent_rs_path.parent().unwrap_or(Path::new(""));
        let file_stem = parent_rs_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let candidates = if file_stem == "mod" || file_stem == "lib" || file_stem == "main" {
            [
                parent_dir.join(format!("{mod_name}.rs")),
                parent_dir.join(format!("{mod_name}/mod.rs")),
            ]
        } else {
            // foo.rs -> foo/ directory sibling modules
            let sibling_dir = parent_dir.join(file_stem);
            [
                sibling_dir.join(format!("{mod_name}.rs")),
                sibling_dir.join(format!("{mod_name}/mod.rs")),
            ]
        };

        candidates.iter().any(|p| self.should_keep(p))
    }
}
