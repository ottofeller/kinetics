use crate::error::Error;
use crate::project::config_file::ConfigFile;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct RemoveCommand {
    /// Domain name (e.g. example.com)
    #[arg()]
    domain: String,
}

impl Runnable for RemoveCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        RemoveRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct RemoveRunner<'a> {
    command: RemoveCommand,
    writer: &'a Writer,
}

impl Runner for RemoveRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        let mut config = ConfigFile::from_path(project.path.clone())?;
        config.update_domain(None).save()?;

        self.writer.text(&format!(
            "\n{}\nThe domain will be removed on next {}.\n\n",
            console::style(format!(
                "Domain {} removed from kinetics.toml.",
                self.command.domain
            ))
            .green()
            .bold(),
            console::style("kinetics deploy").cyan(),
        ))?;

        self.writer.json(json!({
            "domain": self.command.domain,
        }))?;

        Ok(())
    }
}
