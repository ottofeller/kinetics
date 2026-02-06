use std::str::FromStr;
use crate::commands::invoke::InvokeRunner;
use crate::function::Function;
use crate::project::Project;
use crate::runner::Runner;
use color_eyre::owo_colors::OwoColorize;
use eyre::WrapErr;
use std::collections::HashMap;
use std::path::{Path};

impl InvokeRunner {
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

        println!(
            "\n{} {} {}...",
            console::style("Invoking remote function").green().bold(),
            console::style("from").dimmed(),
            console::style(&display_path).underlined().bold()
        );

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

        println!("{}\n", console::style(&url).dimmed());

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
            .body(self.command.payload.clone().unwrap_or("{}".into()).to_string())
            .send()
            .await
            .wrap_err("Failed to call function URL")?;

        let status = response.status();

        let response_text = response
            .text()
            .await
            .unwrap_or("Failed to read response".to_string());

        println!("Status\n{}\n", status);
        println!("Response\n{}", response_text);
        Ok(())
    }
}
