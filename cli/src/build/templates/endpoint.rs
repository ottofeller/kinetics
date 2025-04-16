pub fn endpoint(import_statement: &str, rust_function_name: &str, is_local: bool) -> String {
    if is_local {
        format!(
            "{import_statement}
            use lambda_http::Request;
            use serde_json;

            #[tokio::main]
            async fn main() -> Result<(), lambda_http::Error> {{\n\
                let queues = std::collections::HashMap::new();
                let mut secrets = std::collections::HashMap::new();

                for (k, v) in std::env::vars() {{
                    if k.starts_with(\"KINETICS_SECRET_\") {{
                        let key = k.replace(\"KINETICS_SECRET_\", \"\");
                        secrets.insert(key, v);
                    }}
                }}

                println!(\"Serving requests\");

                // Construct a mock event with JSON payload
                let payload = serde_json::json!({{
                    \"name\": \"aaa\"
                }});
                let body = serde_json::to_string(&payload).unwrap();
                let event = Request::new(body.into());

                {rust_function_name}(event, &secrets, &queues).await?;
                Ok(())
            }}\n\n"
        )
    } else {
        format!(
            "{import_statement}
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

                println!(\"Provisioning queues\");
                let mut queues = std::collections::HashMap::new();

                for (k, v) in std::env::vars() {{
                    if k.starts_with(\"KINETICS_QUEUE_\") {{
                        let queue_client = aws_sdk_sqs::Client::new(&config)
                            .send_message()
                            .queue_url(v);

                        queues.insert(k.replace(\"KINETICS_QUEUE_\", \"\"), queue_client);
                    }}
                }}

                println!(\"Serving requests\");

                run(service_fn(|event| async {{
                    match {rust_function_name}(event, &secrets, &queues).await {{
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
