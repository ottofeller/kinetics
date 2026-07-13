use crate::project::ConfigFile;
use cargo_metadata::MetadataCommand;
use kinetics_parser::Package;
use std::path::{Path, PathBuf};

// Workspace definition for a project.
//
// A non-workspace project would have one member
// with it's path identical to workspace_root.
//
// Otherwise it's a real workspace.
#[derive(Debug, Default, Clone)]
pub struct Workspace {
    pub root_path: PathBuf,
    pub is_standalone_crate: bool,
    pub packages: Vec<Package>,
}

impl Workspace {
    pub fn from_path(path: &Path) -> eyre::Result<Self> {
        let metadata = MetadataCommand::new().current_dir(path).exec()?;
        let root_path = metadata.workspace_root.as_std_path();
        let packages: Vec<Package> = metadata
            .workspace_members
            .into_iter()
            .filter_map(|member| {
                metadata
                    .packages
                    .iter()
                    .find(|pkg| pkg.id == member)
                    // Convert [cargo_metadata::Package] into a simpler representation
                    // keeping only necessary data (in order to avoid these transforms at consuming code):
                    // - the name from Cargo.toml or the resolved project name for standalone/member projects
                    // - the relative path from workspace root to the package dir.
                    //
                    // The closure is used instead of impl From
                    // in order to access metadata from outer scope.
                    .and_then(|pkg: &cargo_metadata::Package| -> Option<Package> {
                        Some(Package {
                            relative_path: pkg
                                .manifest_path
                                .strip_prefix(root_path)
                                .ok()?
                                .parent()? // Remove filename and keep only the dir name.
                                .into(),
                            name: ConfigFile::from_path(pkg.manifest_path.parent()?.as_std_path())
                                .ok()?
                                .project
                                .name,
                        })
                    })
            })
            .collect();

        // For a standalone crate Workspace construct errors if kinetics.toml exists.
        // The reason is that in this case the config is present at root and in the member
        // (since they are the same entity), and a check for no config conflicts fails.
        let is_standalone_crate = packages.len() == 1
            && packages
                .first()
                .is_some_and(|pkg| root_path.join(&pkg.relative_path) == root_path);

        Ok(Self {
            packages,
            root_path: root_path.into(),
            is_standalone_crate,
        })
    }
}
