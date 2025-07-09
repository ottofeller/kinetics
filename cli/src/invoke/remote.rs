use crate::function::Function;
use eyre::Context;

/// Resolve function name into URL and call it remotely
pub async fn invoke(function: &Function, payload: &str, headers: &str) -> eyre::Result<()> {
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
        .post(function.url()?)
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
