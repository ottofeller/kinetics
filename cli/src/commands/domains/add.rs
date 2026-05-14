use crate::api::domains::create::{Request, Response};
use crate::api::request::Validate;
use crate::error::Error;
use crate::project::Project;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use eyre::Context as _;
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut};

#[derive(clap::Args, Clone)]
pub(crate) struct AddCommand {
    /// Domain name to create (e.g. example.com)
    #[arg(value_name = "DOMAIN")]
    domain_name: String,

    /// Relative path to the project directory
    #[arg(long)]
    project: Option<PathBuf>,
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
    /// Attaches a custom domain to the project
    /// Prints domain status and the nameservers to set at the registrar
    async fn run(&mut self) -> Result<(), Error> {
        let project = self.project(&self.command.project).await?;
        let client = self.api_client().await?;

        let request = Request {
            project_name: project.name.clone(),
            domain_name: self.command.domain_name.clone(),
        };

        if let Some(errors) = request.validate() {
            return Err(Error::new("Validation failed", Some(&errors.join("\n"))));
        }

        self.writer.text(&format!(
            "\n{} {}...\n\n",
            console::style("Adding domain").bold().green(),
            console::style(&self.command.domain_name).bold(),
        ))?;

        let response = client
            .post("/domains/create")
            .json(&request)
            .send()
            .await
            .wrap_err("Failed to call /domains/create endpoint")
            .map_err(|e| self.server_error(Some(e.into())))?;

        if !response.status().is_success() {
            log::error!(
                "Failed to add domain ({}): {}",
                response.status(),
                response.text().await.unwrap_or("Unknown error".to_string()),
            );

            return Err(self.server_error(None));
        }

        let resp: Response = response
            .json()
            .await
            .inspect_err(|e| log::error!("Failed to parse response: {e}"))
            .wrap_err("Invalid response from server")
            .map_err(|e| self.server_error(Some(e.into())))?;

        self.writer.text(&format!(
            "{} {}\n",
            console::style("Status:").dim(),
            console::style(resp.status.to_string()).bold(),
        ))?;

        self.writer.text(&format!(
            "{}\n",
            console::style("Add these nameservers at your domain registrar:").dim(),
        ))?;

        for ns in &resp.nameservers {
            self.writer
                .text(&format!("  {}\n", console::style(ns).bold()))?;
        }

        save_domain(&project, &self.command.domain_name)
            .map_err(|e| self.error(None, None, Some(e.into())))?;

        self.writer.json(json!({
            "success": true,
            "status": resp.status.to_string(),
            "nameservers": resp.nameservers,
        }))?;

        Ok(())
    }
}

fn save_domain(project: &Project, domain: &str) -> eyre::Result<()> {
    let config_path = project.path.join("kinetics.toml");
    let config_content = match fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            format!("[project]\nname = \"{}\"\n", project.name)
        }
        Err(error) => {
            return Err(error).wrap_err(Error::new(
                &format!("Failed to read {}", config_path.display()),
                None,
            ));
        }
    };

    let mut doc = config_content
        .parse::<DocumentMut>()
        .wrap_err("Failed to parse kinetics.toml")?;

    doc["domain"] = value(domain);

    write_config(&config_path, &doc.to_string())
}

fn write_config(config_path: &Path, content: &str) -> eyre::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(config_path)
        .wrap_err(Error::new(
            &format!("Failed to open {}", config_path.display()),
            None,
        ))?;

    file.write_all(content.as_bytes()).wrap_err(Error::new(
        &format!("Failed to write to {}", config_path.display()),
        None,
    ))?;

    Ok(())
}
