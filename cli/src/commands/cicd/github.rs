use crate::{error::Error, project::Project};
use eyre::WrapErr;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const GITHUB_WORKFLOW_TEMPLATE: &str = include_str!("github-workflow-template.yaml");

/// Add a GitHub CD workflow
pub fn github_workflow(project: &Project) -> eyre::Result<()> {
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

    let rel_path = project
        .path
        .strip_prefix(&git_root)?
        .to_str()
        .ok_or(Error::new(
            "Failed constructing project path relative to git root",
            Some("Check the path to contain only UTF-8 symbols."),
        ))?;

    let github_workflow = GITHUB_WORKFLOW_TEMPLATE
        .replace(
            "PLACEHOLDER_DIR_PATH",
            if rel_path.is_empty() { "." } else { rel_path },
        )
        .replace(
            "tool: kinetics",
            &format!("tool: kinetics@{}", env!("CARGO_PKG_VERSION")),
        );
    let workflow_dir = git_root.join(".github/workflows");
    fs::create_dir_all(&workflow_dir)
        .inspect_err(|e| log::error!("{e:?}"))
        .wrap_err(Error::new(
            "Failed to create github workflows directory",
            Some("Check file system permissions."),
        ))?;

    let deploy_workflow_filename = if git_root == project.path {
        "kinetics.yaml".into()
    } else {
        format!("kinetics-{}", project.name)
    };
    let deploy_workflow_path = workflow_dir.join(deploy_workflow_filename);
    fs::write(&deploy_workflow_path, github_workflow).wrap_err(Error::new(
        "Failed to write deploy workflow file",
        Some("Check file system permissions."),
    ))?;

    println!(
        "\n{}\n{}\n",
        console::style("A github workflow was added to the project, requires configuration").dim(),
        console::style(
            "https://github.com/ottofeller/kinetics/blob/main/README.md#deploy-from-github-actions"
        )
        .cyan()
    );
    Ok(())
}
