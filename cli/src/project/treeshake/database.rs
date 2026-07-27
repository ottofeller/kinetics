use eyre::{Context, ContextCompat};
use ra_ap_base_db::{
    CrateGraphBuilder, CrateOrigin, CrateWorkspaceData, Env, FileId, FileSet, SourceRoot, VfsPath,
};
use ra_ap_hir::ChangeWithProcMacros;
use ra_ap_ide::{AnalysisHost, Edition};
use ra_ap_paths::{AbsPathBuf, Utf8PathBuf};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use triomphe::Arc;
use walkdir::WalkDir;

/// Maps physical Rust source paths to the file IDs registered in rust-analyzer.
#[derive(Debug)]
pub(super) struct SourceIndex {
    /// Resolves analysis file IDs back to physical source paths.
    id_to_path: HashMap<FileId, PathBuf>,
    /// Resolves physical source paths to analysis file IDs.
    path_to_id: HashMap<PathBuf, FileId>,
}

impl SourceIndex {
    /// Returns the physical path registered for an analysis file.
    pub(super) fn path(&self, file_id: FileId) -> Option<&Path> {
        self.id_to_path.get(&file_id).map(PathBuf::as_path)
    }

    /// Returns the analysis file registered for a physical path.
    pub(super) fn file_id(&self, path: &Path) -> Option<FileId> {
        self.path_to_id.get(path).copied()
    }

    /// Iterates over all registered files.
    pub(super) fn files(&self) -> impl Iterator<Item = (FileId, &Path)> {
        self.id_to_path
            .iter()
            .map(|(file_id, path)| (*file_id, path.as_path()))
    }

    /// Reports whether an analysis file belongs to this package source root.
    pub(super) fn contains(&self, file_id: FileId) -> bool {
        self.id_to_path.contains_key(&file_id)
    }
}

/// Owns the manually loaded rust-analyzer database and its source index.
#[derive(Debug)]
pub(super) struct AnalysisDatabase {
    /// Provides semantic analysis over the registered package source.
    pub(super) host: AnalysisHost,
    /// Maps rust-analyzer file IDs to physical package paths in both directions.
    pub(super) source_index: SourceIndex,
    /// Absolute package root used for source lookup and proc-macro expansion.
    pub(super) package_root: PathBuf,
    /// Physical `src/lib.rs` or `src/main.rs` selected as the analysis crate root.
    pub(super) crate_root_path: PathBuf,
}

impl AnalysisDatabase {
    /// Loads every Rust source file in a package into a deterministic local source root.
    pub(super) fn load(package_root: &Path) -> eyre::Result<Self> {
        let package_root = absolute_path(package_root)?;
        let mut rust_paths = Vec::new();
        let ignored_paths = [
            package_root.join("target"),
            package_root.join(".git"),
            package_root.join(".github"),
        ];

        for entry in WalkDir::new(&package_root)
            .into_iter()
            .filter_entry(|entry| {
                !ignored_paths
                    .iter()
                    .any(|ignored_path| entry.path().starts_with(ignored_path))
            })
        {
            let entry = entry
                .wrap_err_with(|| format!("Failed to walk Rust sources under {package_root:?}"))?;
            if entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "rs")
            {
                rust_paths.push(entry.into_path());
            }
        }
        rust_paths.sort();

        let root_path = [
            package_root.join("src/lib.rs"),
            package_root.join("src/main.rs"),
        ]
        .into_iter()
        .find(|candidate| rust_paths.binary_search(candidate).is_ok())
        .wrap_err_with(|| {
            format!("Package {package_root:?} has neither src/lib.rs nor src/main.rs")
        })?;

        let mut file_set = FileSet::default();
        let mut id_to_path = HashMap::new();
        let mut path_to_id = HashMap::new();
        let mut file_contents = Vec::with_capacity(rust_paths.len());

        for (raw_file_id, path) in (0u32..).zip(rust_paths) {
            let content = fs::read_to_string(&path)
                .wrap_err_with(|| format!("Failed to read Rust source {path:?}"))?;
            let file_id = FileId::from_raw(raw_file_id);
            let vfs_path = VfsPath::new_real_path(
                path.to_str()
                    .wrap_err_with(|| format!("Rust source path is not valid UTF-8: {path:?}"))?
                    .to_owned(),
            );

            file_set.insert(file_id, vfs_path);
            id_to_path.insert(file_id, path.clone());
            path_to_id.insert(path, file_id);
            file_contents.push((file_id, content));
        }

        let root_file_id = path_to_id
            .get(&root_path)
            .copied()
            .wrap_err("Selected crate root was not registered")?;
        let proc_macro_cwd = Arc::new(abs_utf8_path(&package_root)?);
        let mut crate_graph = CrateGraphBuilder::default();
        crate_graph.add_crate_root(
            root_file_id,
            Edition::CURRENT,
            None,
            None,
            Default::default(),
            None,
            Env::default(),
            CrateOrigin::Local {
                repo: None,
                name: None,
            },
            false,
            proc_macro_cwd,
            Arc::new(CrateWorkspaceData {
                target: Err("no layout".into()),
                toolchain: None,
            }),
        );

        let source_root = SourceRoot::new_local(file_set);
        let mut change = ChangeWithProcMacros::default();
        change.set_roots(vec![source_root]);
        for (file_id, content) in file_contents {
            change.change_file(file_id, Some(content));
        }
        change.set_crate_graph(crate_graph);

        let mut host = AnalysisHost::new(None);
        host.apply_change(change);

        log::debug!(
            "loaded {} Rust sources from {package_root:?}, crate root {root_path:?}",
            id_to_path.len()
        );

        Ok(Self {
            host,
            source_index: SourceIndex {
                id_to_path,
                path_to_id,
            },
            package_root,
            crate_root_path: root_path,
        })
    }
}

/// Makes a package path absolute without resolving symlinks used by later copy operations.
fn absolute_path(path: &Path) -> eyre::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .wrap_err("Failed to resolve the current directory")?
            .join(path))
    }
}

/// Converts an absolute std path into rust-analyzer's UTF-8 path type.
fn abs_utf8_path(path: &Path) -> eyre::Result<AbsPathBuf> {
    let utf8 = Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| eyre::eyre!("Package path is not valid UTF-8: {path:?}"))?;
    AbsPathBuf::try_from(utf8).map_err(|path| eyre::eyre!("Package path is not absolute: {path}"))
}
