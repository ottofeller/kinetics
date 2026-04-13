use crate::error::Error;
use crate::project::config_file::ConfigFile;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct AddCommand {
    /// Domain name (e.g. example.com)
    #[arg()]
    name: String,
}

impl Runnable for AddCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        AddRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct AddRunner<'a> {
    command: AddCommand,
    writer: &'a Writer,
}

impl Runner for AddRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project().await?;

        let mut config = ConfigFile::from_path(project.path.clone())?;
        config.update_domain(Some(&self.command.name)).save()?;

        let nameservers: Vec<String> = (1..=4)
            .map(|i| {
                format!(
                    "  {}",
                    console::style(format!("ns{i}.kineticscloud.com")).bold()
                )
            })
            .collect();

        self.writer.text(&format!(
            "\n{}\n\n\
            Update your domain's nameservers at your registrar:\n\n{}\n\n\
            DNS propagation may take up to 48 hours.\n\
            The domain will be deployed on next {}.\n\n",
            console::style(format!(
                "Domain {} added to kinetics.toml.",
                self.command.name
            ))
            .green()
            .bold(),
            nameservers.join("\n"),
            console::style("kinetics deploy").cyan(),
        ))?;

        self.writer.json(json!({
            "domain": self.command.name,
        }))?;

        Ok(())
    }
}
