use crate::api::db::connect::{Request as ConnectRequest, Response as ConnectResponse};
use crate::client::Client;
use crate::function::Function;
use crate::migrations::Migrations;
use crate::project::Project;
use color_eyre::owo_colors::OwoColorize;
use eyre::Context;
use std::path::Path;

/// Resolve function name into URL and call it remotely
pub async fn invoke(
    function: &Function,
    project: &Project,
    payload: Option<&str>,
    headers: Option<&str>,
    url_path: Option<&str>,
    is_migrations_enabled: bool,
    migrations_path: Option<&str>,
) -> eyre::Result<()> {
    let home = std::env::var("HOME").wrap_err("Can not read HOME env var")?;
    let invoke_dir = Path::new(&home).join(format!(".kinetics/{}", project.name));
    let display_path = format!("{}/src/bin/{}.rs", invoke_dir.display(), function.name);

    if is_migrations_enabled {
        println!(
            "{}",
            console::style("Applying migrations...").green().bold()
        );

        let response = Client::new(false)
            .await?
            .request::<_, ConnectResponse>(
                "/stack/sqldb/connect",
                ConnectRequest {
                    project: project.name.clone(),
                },
            )
            .await
            .wrap_err("Failed to get SQL DB connection string")?;

        // FIXME Move create migrations table routine
        let connection = sqlx::PgPool::connect(&response.connection_string).await?;

        sqlx::query(
            r#"
             CREATE TABLE IF NOT EXISTS schema_migrations (
                id VARCHAR(255) PRIMARY KEY,
                applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
        "#,
        )
        .execute(&connection)
        .await?;

        let path = project.path.join(migrations_path.unwrap_or("migrations"));
        let migrations = Migrations::new(path.as_path())?;
        migrations.apply(response.connection_string).await?;

        println!(
            "{}\n",
            console::style("Migrations applied successfully")
                .green()
                .bold()
        );
    }

    println!(
        "\n{} {} {}...",
        console::style("Invoking remote function").green().bold(),
        console::style("from").dimmed(),
        console::style(&display_path).underlined().bold()
    );

    // `url_path` arg is optional,
    // thus fall back to the url_path from macro
    // in order to call correct function.
    let url = if url_path.is_none_or(|p| p.is_empty()) {
        // Replace templating characters as they are not a part of a URL.
        function.url().await?.replace(['{', '}', '+', '*'], "")
    } else {
        format!(
            "{}/{}",
            Project::fetch_one(&function.project.name).await?.url,
            url_path.unwrap()
        )
    };

    println!("{}\n", console::style(&url).dimmed());

    // Parse headers string into HeaderMap
    let mut headers_map = reqwest::header::HeaderMap::new();

    if let Some(headers) = headers {
        for header_line in headers.lines() {
            if let Some((key, value)) = header_line.split_once(':') {
                if let (Ok(header_name), Ok(header_value)) = (
                    reqwest::header::HeaderName::from_bytes(key.trim().as_bytes()),
                    reqwest::header::HeaderValue::from_str(value.trim()),
                ) {
                    headers_map.insert(header_name, header_value);
                }
            } else {
                log::warn!("Unsupported http header format: {header_line}");
            }
        }
    }

    let client = reqwest::Client::new();

    let response = client
        .post(url)
        .headers(headers_map)
        .body(payload.unwrap_or("{}").to_string())
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
