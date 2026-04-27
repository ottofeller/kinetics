use cargo_metadata::MetadataCommand;
use kinetics_parser::Package;
use std::path::{Path, PathBuf};

use crate::project::ConfigFile;

// Workspace definition for a project.
//
// A non-workspace project would have one member
// with it's path identical to workspace_root.
//
// Otherwise it's a real workspace.
#[derive(Debug, Default, Clone)]
pub struct Workspace {
    pub root_path: PathBuf,
    pub packages: Vec<Package>,
}

impl Workspace {
    pub fn from_path(path: &Path) -> eyre::Result<Self> {
        let metadata = MetadataCommand::new().current_dir(path).exec()?;
        let convert_package = |pkg: &cargo_metadata::Package| -> Option<Package> {
            Some(Package {
                name: ConfigFile::cargo_toml_name(pkg.manifest_path.parent()?.as_std_path())
                    .ok()?,
                relative_path: pkg
                    .manifest_path
                    .strip_prefix(&metadata.workspace_root)
                    .ok()?
                    .parent()? // Remove filename and keep only the dir name.
                    .into(),
            })
        };

        // Validate workspace rules for kinetics:
        // 1. Workspace as a single kinetics project:
        //  - the workspace root MUST contain kinetics.toml;
        //  - workspace members cannot have kinetics.toml - throw an error.
        // 2. Standalone kinetics project:
        //  - the project can contain kinetics.toml;
        //  - for the name fall back to Cargo.toml.
        // 3. Workspace member is a kinetics project:
        //  - workspace member from where the command is called MUST have kinetics.toml.
        //  - the workspace root cannot have kinetics.toml - throw an error.
        let workspace_config = metadata.workspace_root.join("kinetics.toml");

        // Option 3. A call within a workspace member which is a kinetics project
        if metadata.workspace_root != path && path.join("kinetics.toml").exists() {
            if workspace_config.exists() {
                eyre::bail!("Workspace is not allowed to have `kinetics.toml` within its root and within its members at the same time.");
            }

            // Return `Workspace` with only one package in the `packages` list - current project.
            let cwd_manifest = path.join("Cargo.toml");
            return Ok(Self {
                packages: metadata
                    .workspace_members
                    .into_iter()
                    .filter_map(|member| {
                        metadata
                            .packages
                            .iter()
                            .find(|pkg| pkg.id == member)
                            .and_then(|pkg| {
                                // Take only the member corresponding to cwd - current project
                                // and discard all other members.
                                if pkg.manifest_path == cwd_manifest {
                                    convert_package(pkg)
                                } else {
                                    None
                                }
                            })
                    })
                    .collect(),
                root_path: metadata.workspace_root.into_std_path_buf(),
            });
        }

        // Options 1 and 2. A call from root.
        let members_configs: Vec<_> = metadata
            .workspace_members
            .iter()
            .filter_map(|member| {
                metadata
                    .packages
                    .iter()
                    .find(|pkg| pkg.id == *member)
                    .and_then(|pkg| pkg.manifest_path.parent())
                    .and_then(|dir| {
                        let config = dir.join("kinetics.toml");
                        if config.exists() {
                            Some(config)
                        } else {
                            None
                        }
                    })
            })
            .collect();

        if workspace_config.exists() && !members_configs.is_empty() {
            eyre::bail!("Workspace is not allowed to have `kinetics.toml` within its root and within its members at the same time.");
        }

        Ok(Self {
            packages: metadata
                .workspace_members
                .into_iter()
                .filter_map(|member| {
                    metadata
                        .packages
                        .iter()
                        .find(|pkg| pkg.id == member)
                        .and_then(convert_package)
                })
                .collect(),
            root_path: metadata.workspace_root.into_std_path_buf(),
        })
    }
}
