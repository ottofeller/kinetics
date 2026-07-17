use super::DependencyGraph;
use kinetics_parser::ParsedFunction;
use ra_ap_base_db::{
    CrateGraphBuilder, CrateOrigin, CrateWorkspaceData, Env, FileId, FileSet, SourceRoot, VfsPath,
};
use ra_ap_hir::ChangeWithProcMacros;
use ra_ap_ide::{AnalysisHost, Edition};
use ra_ap_paths::AbsPathBuf;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use triomphe::Arc;
use walkdir::WalkDir;

/// Builds the `rust-analyzer` database used to analyze a package's Rust source files
#[derive(Debug)]
pub(crate) struct TreeShakerBuilder {
    /// Virtual file registry that defines the local source root loaded into the database.
    file_set: FileSet,
    /// Maps assigned analysis file IDs back to their physical source paths.
    id_to_path: HashMap<FileId, PathBuf>,
}

impl TreeShakerBuilder {
    pub(crate) fn new() -> Self {
        Self {
            file_set: FileSet::default(),
            id_to_path: HashMap::new(),
        }
    }

    /// Build the dependency graph by
    /// - scanning the source directory;
    /// - apply all files as changes to register their content within the graph.
    pub(crate) fn build(mut self, src_dir: &Path) -> eyre::Result<TreeShaker> {
        let mut file_id_counter = 0u32;
        let mut file_contents = HashMap::new();

        log::debug!("walk with TreeShakerBuilder: {src_dir:?}");
        // 1. Recursively scan for .rs files and collect contents
        let mut file_count = 0usize;
        for entry in WalkDir::new(src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
        {
            let path = entry.path();
            if let Ok(content) = fs::read_to_string(path) {
                let vfs_path = VfsPath::new_virtual_path(path.to_string_lossy().into_owned());
                let fid = FileId::from_raw(file_id_counter);

                self.file_set.insert(fid, vfs_path);
                self.id_to_path.insert(fid, path.to_path_buf());

                file_contents.insert(fid, content);

                file_id_counter += 1;
                file_count += 1;
            }
        }
        log::debug!("TreeShakerBuilder: found {file_count} .rs files");

        // 2. Build SourceRoot and CrateGraph
        let source_root = SourceRoot::new_local(self.file_set.clone());
        let mut crate_graph = CrateGraphBuilder::default();

        // Find the root of the package (src/lib.rs or src/main.rs)
        let root_file_id = self.id_to_path.iter().find_map(|(&id, path)| {
            if path.ends_with("src/lib.rs") || path.ends_with("src/main.rs") {
                Some(id)
            } else {
                None
            }
        });

        if let Some(fid) = root_file_id {
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

        // 3. Create AnalysisHost and apply changes
        let mut host = AnalysisHost::new(None);
        let mut change = ChangeWithProcMacros::default();
        change.set_roots(vec![source_root.clone()]);
        for (fid, content) in &file_contents {
            change.change_file(*fid, Some(content.clone()));
        }
        change.set_crate_graph(crate_graph);

        host.apply_change(change);

        Ok(TreeShaker {
            id_to_path: self.id_to_path,
            host,
        })
    }
}

/// Owns the initialized analysis database used to compute per-function dependencies.
#[derive(Debug)]
pub(crate) struct TreeShaker {
    /// Maps registered analysis file IDs to the source paths emitted after shaking.
    id_to_path: HashMap<FileId, PathBuf>,
    /// Provides semantic resolution for dependency graph construction.
    host: AnalysisHost,
}

impl TreeShaker {
    /// Build a DependencyGraph for the given function, referencing this TreeShaker.
    pub(crate) fn dependency_graph(
        &self,
        parsed_function: &ParsedFunction,
    ) -> eyre::Result<DependencyGraph<'_>> {
        let mut graph = DependencyGraph::new(&self.host, &self.id_to_path);
        graph.build_from(parsed_function)?;
        Ok(graph)
    }
}
