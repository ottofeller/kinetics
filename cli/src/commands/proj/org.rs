use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;
use serde_json::json;
use std::fs;
use toml_edit::{value, DocumentMut, Table};

#[derive(clap::Args, Clone)]
pub(crate) struct OrgCommand {
    /// The organization name to set
    name: String,
}

impl Runnable for OrgCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        OrgRunner {
            name: self.name.clone(),
            writer,
        }
    }
}

struct OrgRunner<'a> {
    name: String,
    writer: &'a Writer,
}

impl Runner for OrgRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        let path = "kinetics.toml";

        let content = if fs::metadata(path).is_ok() {
            let existing = fs::read_to_string(path).map_err(|e| {
                self.error(
                    Some("Failed to read kinetics.toml"),
                    Some("Check file permissions."),
                    Some(e.into()),
                )
            })?;

            let mut doc = existing.parse::<DocumentMut>().map_err(|e| {
                self.error(
                    Some("Failed to parse kinetics.toml"),
                    Some("The file contains invalid TOML."),
                    Some(e.into()),
                )
            })?;

            if doc.get("project").is_none() {
                doc["project"] = toml_edit::Item::Table(Table::new());
            }

            doc["project"]["org"] = value(&self.name);
            doc.to_string()
        } else {
            format!("[project]\norg = \"{}\"\n", self.name)
        };

        fs::write(path, &content).map_err(|e| {
            self.error(
                Some("Failed to write kinetics.toml"),
                Some("Check file permissions."),
                Some(e.into()),
            )
        })?;

        self.writer.text(&format!(
            "{} {}\n",
            console::style("Organization set to").green().bold(),
            console::style(&self.name).bold()
        ))?;

        self.writer
            .json(json!({"success": true, "org": self.name}))?;

        Ok(())
    }
}
