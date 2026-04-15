use crate::error::Error;
use crate::project::config_file::ConfigFile;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use serde_json::json;

#[derive(clap::Args, Clone)]
pub(crate) struct RemoveCommand;

impl Runnable for RemoveCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        RemoveRunner { writer }
    }
}

struct RemoveRunner<'a> {
    writer: &'a Writer,
}

impl Runner for RemoveRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let mut project = self.project().await?;

        let domain_name = project
            .domain_name
            .as_ref()
            .ok_or_else(|| {
                self.error(
                    Some("No domain configured"),
                    Some("There is no domain to remove"),
                    None,
                )
            })?
            .clone();

        let mut config = ConfigFile::from_path(project.path.clone())
            .map_err(|e| self.server_error(Some(e.into())))?;

        config
            .set_domain_name(None)
            .save()
            .map_err(|e| self.server_error(Some(e.into())))?;

        project.domain_name = None;

        // Mark all functions as deploying to enable full deploy on backend
        let functions: Vec<_> = project
            .functions()
            .map_err(|e| self.server_error(Some(e.into())))?
            .into_iter()
            .map(|f| f.set_is_deploying(true))
            .collect();

        self.writer.text(&format!(
            "\n{} {} {}...\n",
            console::style("Removing domain").green().bold(),
            console::style("for").dim(),
            console::style(&domain_name).bold(),
        ))?;

        project
            .deploy(&functions, false, None, None)
            .await
            .map_err(|e| self.server_error(Some(e.into())))?;

        let mut status = project
            .status()
            .await
            .map_err(|e| self.server_error(Some(e.into())))?;

        while status.status == "IN_PROGRESS" {
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

            status = project
                .status()
                .await
                .map_err(|e| self.server_error(Some(e.into())))?;
        }

        if matches!(status.status.as_str(), "FAILED" | "FROZEN") {
            let error_text = status
                .errors
                .map(|errors| errors.join("\n"))
                .unwrap_or("Unknown error".into());

            return Err(self.error(Some("Domain removal failed"), Some(&error_text), None));
        }

        self.writer.text(&format!(
            "\n\n{}\n{}\n",
            console::style(format!("Domain {} removed.", domain_name))
                .green()
                .bold(),
            console::style("DNS records and hosted zone have been cleaned up").dim(),
        ))?;

        self.writer.json(json!({
            "domain": domain_name,
        }))?;

        Ok(())
    }
}
