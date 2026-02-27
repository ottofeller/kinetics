use crate::error::Error;
use crate::project::Project;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use color_eyre::owo_colors::OwoColorize;

#[derive(clap::Args, Clone)]
pub(crate) struct ListCommand;

impl Runnable for ListCommand {
    fn runner(&self, _writer: &Writer) -> impl Runner {
        ListRunner {}
    }
}

struct ListRunner;

impl Runner for ListRunner {
    /// Prints out the list of all projects
    async fn run(&mut self) -> Result<(), Error> {
        // Let it fail if user's logged out
        self.api_client().await?;

        println!(
            "{}...\n",
            console::style("Fetching projects").green().bold()
        );

        Project::fetch_all()
            .await
            .map_err(|e| self.server_error(Some(e.into())))?
            .iter()
            .for_each(|Project { name, url, .. }| println!("{}\n{}\n", name.bold(), url.dimmed()));

        Ok(())
    }
}
