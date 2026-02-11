use crate::commands::cicd::github;
use crate::error::Error;
use crate::runner::{Runnable, Runner};

#[derive(clap::Args, Clone)]
pub(crate) struct InitCommand {
    /// Create a GitHub workflow file.
    #[arg(short, long, action = clap::ArgAction::SetTrue, required = false)]
    github: bool,
}

impl Runnable for InitCommand {
    fn runner(&self) -> impl Runner {
        InitRunner {
            command: self.clone(),
        }
    }
}

struct InitRunner {
    command: InitCommand,
}

impl Runner for InitRunner {
    /// Initialize a deployment workflow within an existing kinetics project
    ///
    /// Currently only supports GitHub, but expected to expand in the future.
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        println!(
            "{}",
            console::style("Creating GitHub workflow...").bold().green()
        );

        github::workflow(&project, false).map_err(|e| self.error(None, None, Some(e.into())))?;

        println!("{}", console::style("Done").bold().green());
        Ok(())
    }
}
