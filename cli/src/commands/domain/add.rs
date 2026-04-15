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
        let mut project = self.project().await?;

        if project
            .domain_name
            .as_ref()
            .is_some_and(|d| d.eq(&self.command.name))
        {
            self.writer.text(&format!(
                "\n{}\n{}\n\n",
                console::style(format!(
                    "Domain {} is already configured.",
                    self.command.name
                ))
                .yellow()
                .bold(),
                console::style("Run `kinetics domain status` to check its state").dim(),
            ))?;

            return Ok(());
        }

        let mut config = ConfigFile::from_path(project.path.clone())
            .map_err(|e| self.server_error(Some(e.into())))?;

        config
            .set_domain_name(Some(&self.command.name))
            .save()
            .map_err(|e| self.server_error(Some(e.into())))?;

        project.domain_name = Some(self.command.name.clone());

        let functions = project
            .functions()
            .map_err(|e| self.server_error(Some(e.into())))?;

        self.writer.text(&format!(
            "\n{} {} {}...",
            console::style("Provisioning domain").green().bold(),
            console::style("for").dim(),
            console::style(&self.command.name).bold(),
        ))?;

        project
            .deploy(
                &functions,
                false,
                None,
                Some("Applying custom domain".into()),
            )
            .await
            .map_err(|e| self.server_error(Some(e.into())))?;

        let nameservers: Vec<String> = (1..=4)
            .map(|i| format!("  ns{i}.kineticscloud.com"))
            .collect();

        self.writer.text(&format!(
            "\n\n{}\n\n\
            {}\n\n\
            {}\n\n\
            {}\n\
            {}\n",
            console::style(format!("Domain {} added.", self.command.name))
                .green()
                .bold(),
            console::style("Update your domain's nameservers at your registrar:").dim(),
            console::style(nameservers.join("\n")).bold(),
            console::style("DNS propagation may take up to 48 hours").dim(),
            console::style("Run `kinetics domain status` to check").dim(),
        ))?;

        self.writer.json(json!({
            "domain": self.command.name,
        }))?;

        Ok(())
    }
}
