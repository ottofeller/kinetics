use crate::{error::Error, project::Project};
use eyre::WrapErr;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
const GITHUB_WORKFLOW_TEMPLATE: &str = include_str!("github-workflow-template.yaml");

/// Add a GitHub CD workflow
///
/// When is_silent is true no CLI  output generated.
pub fn workflow(project: &Project, is_silent: bool) -> eyre::Result<()> {
    // Resolve the Git root - GitHub workflow shall be added there.
    let git_root = match Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&project.path)
        .output()
    {
        Ok(output) if output.status.success() => {
            PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
        }
        _ => {
            return Err(Error::new(
                "Failed to find git root",
                Some("Ensure the project is within a git tree."),
            )
            .into());
        }
    };

    let rel_path = match project.path.strip_prefix(&git_root)?.to_str() {
        Some("") => ".",
        Some(rel_path) => rel_path,
        None => {
            return Err(Error::new(
                "Failed constructing project path relative to git root",
                Some("Check the path to contain only UTF-8 symbols."),
            )
            .into());
        }
    };

    let github_workflow = GITHUB_WORKFLOW_TEMPLATE
        .replace("PLACEHOLDER_DIR_PATH", rel_path)
        .replace(
            "tool: kinetics",
            &format!("tool: kinetics@{}", env!("CARGO_PKG_VERSION")),
        );
    let workflow_dir = git_root.join(".github/workflows");
    fs::create_dir_all(&workflow_dir)
        .inspect_err(|e| log::error!("Error: {e:?}"))
        .wrap_err(Error::new(
            "Failed to create github workflows directory",
            Some("Check file system permissions."),
        ))?;

    let deploy_workflow_filename = if git_root == project.path {
        // If the project is at git root, we would have only one workflow file.
        "kinetics.yaml".into()
    } else {
        // If the project is within a workspace,
        // there might be multiple kinetics projects and multiple workflows.
        // Thus add project name to avoid filename clashes.
        format!("kinetics-{}.yaml", project.name)
    };
    let deploy_workflow_path = workflow_dir.join(deploy_workflow_filename);

    if fs::exists(&deploy_workflow_path)? {
        return Err(Error::new(
            &format!(
                "Workflow already exists\n{}",
                console::style(deploy_workflow_path.to_string_lossy())
                    .bold()
                    .underlined(),
            ),
            None,
        )
        .into());
    }

    fs::write(&deploy_workflow_path, github_workflow)
        .inspect_err(|e| log::error!("Error: {e:?}"))
        .wrap_err(Error::new(
            "Failed to write deploy workflow file",
            Some("Check file system permissions."),
        ))?;

    if !is_silent {
        println!(
            "\n{}\n{}\n\n{}\n{}\n",
            console::style("Added CI/CD config files at").dim(),
            console::style(deploy_workflow_path.to_string_lossy())
                .bold()
                .underlined(),
            console::style("CI/CD docs available at").yellow(),
            console::style(
                "https://github.com/ottofeller/kinetics/blob/main/README.md#deploy-from-github-actions"
            )
            .cyan(),
        );
    }

    Ok(())
}
