use crate::error::Error;
use crate::project::Project;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use color_eyre::owo_colors::OwoColorize;
use eyre::WrapErr;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(clap::Args, Clone)]
pub(crate) struct ListCommand {
    /// Organization to list projects for. Defaults to the current project's org from kinetics.toml.
    #[arg(long)]
    org: Option<String>,

    /// Relative path to the project directory
    #[arg(long)]
    project: Option<PathBuf>,
}

impl Runnable for ListCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        ListRunner {
            writer,
            org: self.org.clone(),
            project: self.project.clone(),
        }
    }
}

struct ListRunner<'a> {
    writer: &'a Writer,
    org: Option<String>,
    project: Option<PathBuf>,
}

impl Runner for ListRunner<'_> {
    /// Prints out the list of all projects
    async fn run(&mut self) -> Result<(), Error> {
        // Let it fail if user's logged out
        self.api_client().await?;

        self.writer.text(&format!(
            "{}...\n\n",
            console::style("Fetching projects").green().bold()
        ))?;

        let project = self
            .config_project()
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        let org = self
            .org
            .as_deref()
            .or(project.as_ref().and_then(|project| project.org.as_deref()));

        let projects = Project::fetch_all(org)
            .await
            .map_err(|e| self.server_error(Some(e.into())))?;

        if projects.is_empty() {
            self.writer
                .text(&format!("{}", console::style("No projects found").yellow()))?;

            self.writer.json(json!({"success": true, "projects": []}))?;
            return Ok(());
        }

        let mut projects_json: Vec<Value> = vec![];

        for Project { name, url, .. } in &projects {
            self.writer
                .text(&format!("{}\n{}\n\n", name.bold(), url.dimmed()))?;

            projects_json.push(json!({"name": name, "url": url}));
        }

        self.writer
            .json(json!({"success": true, "projects": projects_json}))?;

        Ok(())
    }
}

impl ListRunner<'_> {
    /// Returns the project from the config file, if one exists
    fn config_project(&self) -> eyre::Result<Option<Project>> {
        let path = std::env::current_dir()
            .wrap_err("Failed to get current dir")?
            .join(self.project.clone().unwrap_or_default());

        if !path.join("kinetics.toml").exists() {
            return Ok(None);
        }

        Project::from_path(path).map(Some)
    }
}
