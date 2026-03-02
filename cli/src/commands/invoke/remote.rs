use std::str::FromStr;
use crate::commands::invoke::InvokeRunner;
use crate::function::Function;
use crate::project::Project;
use crate::runner::Runner;
use color_eyre::owo_colors::OwoColorize;
use eyre::WrapErr;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path};

impl InvokeRunner<'_> {
    /// Resolve function name into URL and call it remotely
    #[allow(clippy::too_many_arguments)]
    pub async fn remote(
        &self,
        function: &Function,
    ) -> eyre::Result<()> {
        let project = self.project().await?;
        let home = std::env::var("HOME").wrap_err("Can not read HOME env var")?;
        let invoke_dir = Path::new(&home).join(format!(".kinetics/{}", project.name));
        let display_path = format!("{}/src/bin/{}.rs", invoke_dir.display(), function.name);

        self.writer.text(&format!(
            "\n{} {} {}...\n",
            console::style("Invoking remote function").green().bold(),
            console::style("from").dimmed(),
            console::style(&display_path).underlined().bold()
        )).map_err(|e| eyre::eyre!(e))?;

        // `url_path` arg is optional,
        // thus fall back to the url_path from macro
        // in order to call correct function.
        let url = if self.command.url_path.clone().is_none_or(|p| p.is_empty()) {
            // Replace templating characters as they are not a part of a URL.
            function.url().await?.replace(['{', '}', '+', '*'], "")
        } else {
            format!(
                "{}/{}",
                Project::fetch_one(&function.project.name).await?.url(),
                self.command.url_path.clone().unwrap()
            )
        };

        self.writer.text(&format!("{}\n\n", console::style(&url).dimmed()))
            .map_err(|e| eyre::eyre!(e))?;

        // Parse headers string into HeaderMap
        let mut headers_map = reqwest::header::HeaderMap::new();

        if let Some(headers) = self.command.headers.clone() {
            for (k, v) in serde_json::from_str::<HashMap<String, String>>(&headers)
                .wrap_err("Failed to parse headers JSON object, must be {\"String\": \"String\"}")?
                .iter()
            {
                headers_map.insert(
                    reqwest::header::HeaderName::from_str(k)
                        .wrap_err("Failed to parse header name")?,
                    reqwest::header::HeaderValue::from_str(v)
                        .wrap_err("Failed to parse header value")?,
                );
            }
        }

        let client = reqwest::Client::new();

        let response = client
            .post(url)
            .headers(headers_map)
            .body(self.command.payload.clone().unwrap_or_else(|| "{}".into()))
            .send()
            .await
            .wrap_err("Failed to call function URL")?;

        let status = response.status();

        let response_text = response
            .text()
            .await
            .unwrap_or("Failed to read response".to_string());

        self.writer.text(&format!("Status\n{}\n\nResponse\n{}\n", status, response_text))
            .map_err(|e| eyre::eyre!(e))?;

        self.writer.json(json!({"status": status.as_u16(), "response": response_text}))
            .map_err(|e| eyre::eyre!(e))?;

        Ok(())
    }
}
