use crate::crat::Crate;
use crate::function::Function;
use crate::project::Project;
use color_eyre::owo_colors::OwoColorize;
use eyre::Context;
use std::path::Path;

/// Resolve function name into URL and call it remotely
pub async fn invoke(
    function: &Function,
    crat: &Crate,
    payload: &str,
    headers: &str,
    url_path: &str,
) -> eyre::Result<()> {
    let home = std::env::var("HOME").wrap_err("Can not read HOME env var")?;
    let invoke_dir = Path::new(&home).join(format!(".kinetics/{}", crat.name));
    let display_path = format!("{}/src/bin/{}.rs", invoke_dir.display(), function.name);

    println!(
        "\n{} {} {}...",
        console::style("Invoking remote function").green().bold(),
        console::style("from").dimmed(),
        console::style(&display_path).underlined().bold()
    );

    let url = if url_path.is_empty() {
        function.url().await?.replace(['{', '}', '+', '*'], "")
    } else {
        format!(
            "{}/{}",
            Project::one(&function.crat.name).await?.url,
            url_path
        )
    };

    println!("{}\n", console::style(&url).dimmed());

    // Parse headers string into HeaderMap
    let mut headers_map = reqwest::header::HeaderMap::new();

    if !headers.is_empty() {
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
        .body(payload.to_string())
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
