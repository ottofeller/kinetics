pub fn endpoint(import_statement: &str, rust_function_name: &str, is_local: bool) -> String {
    if is_local {
        format!(
            "{import_statement}
            use lambda_http::Request;
            use serde_json;
            use reqwest::header::{{HeaderName, HeaderValue}};
            use std::str::FromStr;
            use kinetics::tools::KineticsConfig;
            #[tokio::main]
            async fn main() -> Result<(), lambda_http::Error> {{\n\
                let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                let kinetics_config = KineticsConfig::new(&config).await?;
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

                let mut event = Request::new(payload.into());
                let headers = event.headers_mut();

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

                match {rust_function_name}(event, &secrets, &kinetics_config).await {{
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
            use kinetics::tools::KineticsConfig;
            use lambda_http::{{run, service_fn}};\n\
            #[tokio::main]\n\
            async fn main() -> Result<(), lambda_http::Error> {{\n\
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
                        .await?;

                    let result = desc.parameter.unwrap();

                    let tags = secrets_client
                        .list_tags_for_resource()
                        .resource_type(aws_sdk_ssm::types::ResourceTypeForTagging::Parameter)
                        .resource_id(secret_name.clone())
                        .send()
                        .await?
                        .tag_list
                        .unwrap_or_default();

                    let name = match tags.iter().find(|t| t.key() == \"original_name\") {{
                        Some(tag) => tag.value(),
                        None => &secret_name.clone(),
                    }};

                    let secret_value = result.value().unwrap();
                    secrets.insert(name.into(), secret_value.to_string());
                }}

                let kinetics_config = KineticsConfig::new(&config).await?;
                println!(\"Serving requests\");

                run(service_fn(|event| async {{
                    match {rust_function_name}(event, &secrets, &kinetics_config).await {{
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
