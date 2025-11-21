use color_eyre::owo_colors::OwoColorize;
use eyre::Context;
use std::path::Path;

pub struct Migration<'a> {
    path: &'a Path,
}

impl<'a> Migration<'a> {
    pub async fn new(path: &'a Path) -> eyre::Result<Self> {
        tokio::fs::create_dir_all(path)
            .await
            .wrap_err("Failed to create migrations dir")?;

        Ok(Self { path })
    }

    pub async fn apply(&self) -> eyre::Result<()> {
        Ok(())
    }

    pub async fn create(&self, name: &str) -> eyre::Result<()> {
        let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");

        // Allow only alphanumeric characters and underscores
        let name = name
            .replace(" ", "_")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>();

        let filepath = self.path.join(format!("{}_{}.up.sql", timestamp, name));

        // TODO Add some helpful comments to the migration file
        tokio::fs::write(self.path.join(&filepath), "")
            .await
            .wrap_err("Failed to create migration file")?;

        println!(
            "{}: {}",
            console::style("Migration created successfully")
                .green()
                .bold(),
            console::style(format!(
                "{}/{}",
                filepath
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default(),
                filepath
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            ))
            .dimmed(),
        );

        Ok(())
    }
}
