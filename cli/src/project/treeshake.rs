use ra_ap_base_db::{
    CrateGraphBuilder, CrateOrigin, CrateWorkspaceData, Env, FileId, FileSet, SourceRoot, VfsPath,
};
use ra_ap_hir::ChangeWithProcMacros;
use ra_ap_ide::{Analysis, AnalysisHost, Edition};
use ra_ap_paths::AbsPathBuf;
use std::fs;
use std::path::{Path, PathBuf};
use triomphe::Arc;
use walkdir::WalkDir;

pub struct TreeShaker {
    source_root: Option<SourceRoot>,
    file_set: FileSet,
    id_to_path: std::collections::HashMap<FileId, PathBuf>,
    analysis: Option<Analysis>,
}

impl TreeShaker {
    pub fn new() -> Self {
        Self {
            source_root: None,
            file_set: FileSet::default(),
            id_to_path: std::collections::HashMap::new(),
            analysis: None,
        }
    }

    /// Initialize the dependency graph by
    /// - scanning the source directory;
    /// - apply all files as changes to register their content within the graph.
    pub fn initialize(&mut self, src_dir: &Path) -> eyre::Result<()> {
        let mut file_id_counter = 0u32;
        let mut file_set = FileSet::default();
        let mut id_to_path = std::collections::HashMap::new();
        let mut file_contents = std::collections::HashMap::new();

        // 1. Recursively scan for .rs files and collect contents
        for entry in WalkDir::new(src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "rs"))
        {
            let path = entry.path();
            if let Ok(content) = fs::read_to_string(path) {
                let relative_path = path.strip_prefix(src_dir).unwrap_or(path);
                let vfs_path =
                    VfsPath::new_virtual_path(relative_path.to_string_lossy().into_owned());

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
        let mut root_file_id = None;
        for (&id, path) in id_to_path.iter() {
            if path.ends_with("src/lib.rs") || path.ends_with("src/main.rs") {
                root_file_id = Some(id);
                break;
            }
        }

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
        self.analysis = Some(host.analysis());
        self.source_root = Some(source_root);
        self.file_set = file_set;
        self.id_to_path = id_to_path;

        Ok(())
    }
}

/// Output of the shaken graph, containing a mapping of FileId to a set of retained AstIds.
pub struct PrunedGraph {
    /// Maps file paths (FileId) to a set of names/identities (AstId).
    pub file_to_items: std::collections::HashMap<PathBuf, std::collections::HashSet<String>>,
}

pub struct DependencyGraph {
    // This will house the `rust-analyzer` database and provide reachability analysis.
}

impl DependencyGraph {
    /// Filters out any module, function, or variable whose ID is not in the reached set.
    pub fn prune(&self) -> PrunedGraph {
        todo!("Analysis implementation")
    }
}
