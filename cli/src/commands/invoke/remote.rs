use crate::commands::invoke::InvokeRunner;
use crate::error::Error;
use crate::function::Function;
use crate::project::Project;
use crate::runner::Runner;
use color_eyre::owo_colors::OwoColorize;
use eyre::WrapErr;
use kinetics_api::func;
use kinetics_parser::Role;
use serde_json::json;
use std::collections::HashMap;
use std::str::FromStr;

impl InvokeRunner<'_> {
    /// Resolve function name into URL and call it remotely
    pub async fn remote(&mut self, function: Function) -> eyre::Result<()> {
        match function.role {
            Role::Endpoint => self.endpoint(function).await,
            Role::Cron | Role::Worker => self.worker_or_cron(function).await,
        }
    }

    async fn endpoint(&self, function: Function) -> eyre::Result<()> {
        let payload = self
            .resolve_payload(&function.role)?
            .expect("endpoint payload is always resolved");

        let project = self.project(&self.command.project).await?;
        let display_path = format!(
            "{}/{}/src/bin/{}.rs",
            project.build_path()?.display(),
            function.name,
            function.name
        );

        self.writer
            .text(&format!(
                "\n{} remote function {} {}...\n",
                console::style("Invoking").bold(),
                console::style("from").dimmed(),
                console::style(&display_path).underlined()
            ))
            .map_err(|e| eyre::eyre!(e))?;

        // `url_path` arg is optional,
        // thus fall back to the url_path from macro
        // in order to call correct function.
        let url = match self.command.url_path.as_ref() {
            Some(url_path) if !url_path.is_empty() => {
                format!(
                    "{}/{}",
                    Project::fetch_one(&function.project.name, function.project.org.as_deref())
                        .await?
                        .url(),
                    url_path
                )
            }
            _ => {
                // Replace templating characters as they are not a part of a URL.
                function.url().await?.replace(['{', '}', '+', '*'], "")
            }
        };

        self.writer
            .text(&format!("{}\n\n", console::style(&url).dimmed()))
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
            .body(payload)
            .send()
            .await
            .wrap_err("Failed to call function URL")?;

        let status = response.status();

        let response_text = response
            .text()
            .await
            .unwrap_or("Failed to read response".to_string());

        self.writer
            .text(&format!(
                "Status\n{}\n\nResponse\n{}\n",
                status, response_text
            ))
            .map_err(|e| eyre::eyre!(e))?;

        self.writer
            .json(json!({"status": status.as_u16(), "response": response_text}))
            .map_err(|e| eyre::eyre!(e))?;

        Ok(())
    }

    async fn worker_or_cron(&mut self, function: Function) -> eyre::Result<()> {
        let payload = self.resolve_payload(&function.role)?;
        let project = self.project(&self.command.project).await?;
        let client = self.api_client().await?;

        self.writer.text(&format!(
            "\n{} {}...\n\n",
            console::style("Invoking").bold(),
            function.name
        ))?;

        let response = client
            .post("/function/invoke")
            .json(&func::invoke::Request {
                project: project.into(),
                function_name: function.name,
                payload: match function.role {
                    Role::Worker => payload,
                    Role::Cron => None,
                    _ => unreachable!(),
                },
            })
            .send()
            .await
            .wrap_err("Failed to send invoke request")
            .map_err(|e| self.server_error(Some(e.into())))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or("Unknown error".to_string());
            eyre::bail!("Failed to invoke the function ({status}): {error_text}");
        }

        let body: func::invoke::Response = response.json().await.wrap_err(Error::new(
            "Invalid response from server",
            Some("Try again later."),
        ))?;

        log::debug!("Invoke response: {body:?}");

        if let Some(log) = body.log {
            self.writer.text(&format!(
                "Function logs:\n{}\n",
                console::style(log).yellow(),
            ))?;
        }

        match body.status {
            func::invoke::Status::NotStarted(error) => {
                self.writer
                    .error(&format!("Function not invoked: {error}"))?;
                self.writer
                    .json(json!({"invoked": false, "error": body.payload}))?;
            }
            func::invoke::Status::Success => {
                self.writer.text("Function invoked\n")?;
                self.writer
                    .text(&format!("{}\n", console::style("Success").bold()))?;

                if let Some(payload) = &body.payload {
                    self.writer.text(&format!(
                        "{}",
                        console::style(
                            &String::from_utf8(payload.clone())
                                .unwrap_or_else(|_e| "Not a string".into())
                        )
                        .yellow(),
                    ))?;
                }

                self.writer
                    .json(json!({"invoked": true, "success": true, "payload": body.payload}))?;
            }
            func::invoke::Status::Fail(error) => {
                self.writer.text("Function invoked\n")?;
                self.writer
                    .error(&format!("{} ({})\n", console::style("Error").red(), error))?;
                self.writer.text(&format!(
                    "{}",
                    console::style(
                        &String::from_utf8(
                            body.payload
                                .clone()
                                .unwrap_or_else(|| "Empty payload".into())
                        )
                        .unwrap_or_else(|_e| "Not a string".into())
                    )
                    .yellow(),
                ))?;

                self.writer
                    .json(json!({"invoked": true, "success": false, "payload": body.payload}))?;
            }
        }

        Ok(())
    }
}
