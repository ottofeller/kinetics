use crate::error::Error;
use crate::project::Project;
use crate::runner::{Runnable, Runner};
use crossterm::style::Stylize;
use std::io::{self, Write};

#[derive(clap::Args, Clone)]
pub(crate) struct DestroyCommand {
    /// Name of the project to destroy (optional, defaults to current project name)
    #[arg(short, long)]
    name: Option<String>,
}

impl Runnable for DestroyCommand {
    fn runner(&self) -> impl Runner {
        DestroyRunner {
            command: self.clone(),
        }
    }
}

struct DestroyRunner {
    command: DestroyCommand,
}

impl Runner for DestroyRunner {
    /// Destroys a project after user confirmation
    async fn run(&mut self) -> Result<(), Error> {
        let current_project = self.project().await?;

        let project_name = match &self.command.name {
            Some(name) => name.as_str(),
            None => current_project.name.as_str(),
        };

        let project = match Project::fetch_one(project_name).await {
            Ok(project) => project,
            Err(_) => {
                println!("{}", "Project not found".yellow());
                return Ok(());
            }
        };

        print!("{} {}: ", "Do you want to proceed?".bold(), "[y/N]".dim());
        io::stdout()
            .flush()
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        let mut input = String::new();

        io::stdin()
            .read_line(&mut input)
            .map_err(|e| self.error(Some("Failed to read input"), None, Some(e.into())))?;

        if !matches!(input.trim().to_lowercase().as_ref(), "y" | "yes") {
            println!("{}", "Destroying canceled".dim().bold());
            return Ok(());
        }

        println!("{}: {}", "Destroying".bold(), &project.name);

        project
            .destroy()
            .await
            .map_err(|e| self.error(Some("Failed to destroy project"), None, Some(e.into())))?;

        println!("{}", console::style("Project destroyed").green());
        Ok(())
    }
}
