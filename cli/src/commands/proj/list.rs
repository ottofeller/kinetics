use crate::error::Error;
use crate::project::Project;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use color_eyre::owo_colors::OwoColorize;
use serde_json::{json, Value};

#[derive(clap::Args, Clone)]
pub(crate) struct ListCommand;

impl Runnable for ListCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        ListRunner { writer }
    }
}

struct ListRunner<'a> {
    writer: &'a Writer,
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

        let projects = Project::fetch_all()
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
