use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use clap::ArgAction;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct OrgCommand {
    /// The organization name to set
    org: Option<String>,

    /// Remove the organization from the project
    #[arg(long, action = ArgAction::SetTrue)]
    unset: bool,
}

impl Runnable for OrgCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        OrgRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct OrgRunner<'a> {
    command: OrgCommand,
    writer: &'a Writer,
}

impl Runner for OrgRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        if self.command.unset && self.command.org.is_some() {
            return Err(self.error(
                Some("Conflicting arguments"),
                Some("Provide either an org name or --unset, not both."),
                None,
            ));
        }

        if !self.command.unset && self.command.org.is_none() {
            return Err(self.error(
                Some("Missing argument"),
                Some("Provide an org name or use --unset to remove it."),
                None,
            ));
        }

        let project = self
            .project(&self.command.project)
            .await?
            .with_org(self.command.org.as_ref().map(|o| o.as_str()));

        project.write_config().map_err(|e| {
            self.error(
                Some("Failed to write config file"),
                Some(&e.to_string()),
                Some(e.into()),
            )
        })?;

        match self.command.org.clone() {
            Some(org) => {
                self.writer.text(&format!(
                    "{} {}\n",
                    console::style("Project org set to").green().bold(),
                    console::style(&org).bold()
                ))?;

                self.writer.json(json!({"success": true, "org": org}))?;
            }
            None => {
                self.writer.text(&format!(
                    "{} {}\n",
                    console::style("Project org unset").green().bold(),
                    console::style("").bold()
                ))?;

                self.writer.json(json!({"success": true}))?;
            }
        }

        Ok(())
    }
}
