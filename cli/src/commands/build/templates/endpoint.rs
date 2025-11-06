use crate::tools::config::EndpointConfig;

pub fn endpoint(
    import_statement: &str,
    rust_function_name: &str,
    config: EndpointConfig,
    is_local: bool,
) -> String {
    if is_local {
        format!(
            "{import_statement}
            use http::request::Builder;
            use serde_json;
            use reqwest::header::{{HeaderName, HeaderValue}};
            use std::str::FromStr;
            use kinetics::tools::config::{{Config as KineticsConfig, EndpointConfig}};
            #[tokio::main]
            async fn main() -> Result<(), tower::BoxError> {{\n\
                let user_function = {rust_function_name};
                let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                let endpoint_config = {config};
                let url_path = std::env::var(\"KINETICS_INVOKE_URL_PATH\").unwrap_or_default();
                let url_path = if url_path.is_empty() {{
                    endpoint_config
                        .clone()
                        .url_pattern
                        .unwrap()
                        .replace(['{{', '}}', '+', '*'], \"\")
                }} else {{
                    url_path
                }};
                let kinetics_config = KineticsConfig::new(&config, Some(endpoint_config)).await?;
                let mut secrets = std::collections::HashMap::new();

                for (k, v) in std::env::vars() {{
                    if k.starts_with(\"KINETICS_SECRET_\") {{
                        let key = k.replace(\"KINETICS_SECRET_\", \"\");
                        secrets.insert(key, v);
                    }}
                }}

                let payload = match std::env::var(\"KINETICS_INVOKE_PAYLOAD\") {{
                    Ok(val) => val,
                    Err(_) => \"{{}}\".into(),
                }};

                let headers_json = match std::env::var(\"KINETICS_INVOKE_HEADERS\") {{
                    Ok(val) => val,
                    Err(_) => \"{{}}\".into(),
                }};

                let mut event_builder = Builder::new();
                let headers = event_builder.headers_mut().unwrap();
                let headers_value = serde_json::from_str::<serde_json::Value>(&headers_json)
                    .unwrap_or_default();
                let headers_obj = headers_value.as_object().unwrap();
                for (k, v) in headers_obj.iter() {{
                    headers
                        .insert(
                            HeaderName::from_str(k).unwrap(),
                            HeaderValue::from_str(v.as_str().unwrap()).unwrap(),
                        );
                }}

                let event = event_builder
                    .uri(url_path)
                    .body(kinetics::tools::http::Body::from(payload).try_into()?)?;
                match user_function(event, &secrets, &kinetics_config).await {{
                    Ok(response) => {{
                        println!(\"{{response:?}}\");
                    }},

                    Err(err) => {{
                        println!(\"Request failed: {{:?}}\", err);
                    }}
                }}

                Ok(())
            }}\n\n"
        )
    } else {
        format!(
            "{import_statement}
            use kinetics::tools::config::{{Config as KineticsConfig, EndpointConfig}};
            use lambda_http::{{run, service_fn, Request}};\n\
            #[tokio::main]\n\
            async fn main() -> Result<(), lambda_http::Error> {{\n\
                let user_function = {rust_function_name};
                let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                println!(\"Fetching secrets\");
                let secrets_client = aws_sdk_ssm::Client::new(&config);
                let secrets_names_env = \"KINETICS_SECRETS_NAMES\";
                let mut secrets = std::collections::HashMap::new();

                for secret_name in std::env::var(secrets_names_env)?
                    .split(\",\")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {{
                    let desc = secrets_client
                        .get_parameter()
                        .name(secret_name.clone())
                        .with_decryption(true)
                        .send()
                        .await.inspect_err(|e| {{
                            eprintln!(\"Error fetching secret {{}}: {{:?}}\", secret_name, e);
                        }})?;

                    let result = desc.parameter.unwrap();

                    let tags = secrets_client
                        .list_tags_for_resource()
                        .resource_type(aws_sdk_ssm::types::ResourceTypeForTagging::Parameter)
                        .resource_id(secret_name.clone())
                        .send()
                        .await
                        .inspect_err(|e| {{
                            eprintln!(\"Error fetching tags for secret {{}}: {{:?}}\", secret_name, e);
                        }})?
                        .tag_list
                        .unwrap_or_default();

                    let name = match tags.iter().find(|t| t.key() == \"original_name\") {{
                        Some(tag) => tag.value(),
                        None => &secret_name.clone(),
                    }};

                    let secret_value = result.value().unwrap();
                    secrets.insert(name.into(), secret_value.to_string());
                }}

                let endpoint_config = {config};
                let kinetics_config = KineticsConfig::new(&config, Some(endpoint_config)).await.inspect_err(|e| {{
                    eprintln!(\"Error initializing kinetics config: {{:?}}\", e);
                }})?;

                println!(\"Serving requests\");

                run(service_fn(|event: Request| async {{
                    let (head, body) = event.into_parts();
                    let event = http::Request::from_parts(head, kinetics::tools::http::Body::from(body).try_into()?);
                    match user_function(event, &secrets, &kinetics_config).await {{
                        Ok(response) => Ok(response),
                        Err(err) => {{
                            eprintln!(\"Error occurred while handling request: {{:?}}\", err);
                            Err(err)
                        }}
                    }}
                }})).await
            }}\n\n"
        )
    }
}
