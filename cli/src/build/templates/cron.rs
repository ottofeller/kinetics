pub fn cron(import_statement: &str, rust_function_name: &str, is_local: bool) -> String {
    // For local run we create a dedicated crate, without cargo lambda wrappers
    if is_local {
        format!(
            "{import_statement}
            #[tokio::main]\n\
            async fn main() -> Result<(), Box<dyn std::error::Error>> {{\n\
                let mut queues = std::collections::HashMap::new();
                let mut secrets = std::collections::HashMap::new();

                for (k, v) in std::env::vars() {{
                    if k.starts_with(\"KINETICS_SECRET_\") {{
                        let key = k.replace(\"KINETICS_SECRET_\", \"\");
                        secrets.insert(key, v);
                    }}
                }}

                {rust_function_name}(&secrets, &queues).await?;
                Ok(())
            }}\n\n"
        )
    } else {
        format!(
            "{import_statement}
            use lambda_runtime::{{LambdaEvent, Error, run, service_fn}};\n\
            use kinetics::tools::queue::Client as QueueClient;
            use aws_lambda_events::eventbridge::EventBridgeEvent;\n\
            #[tokio::main]\n\
            async fn main() -> Result<(), Error> {{\n\
                let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                println!(\"Provisioning secrets\");
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
                        let queue_client = QueueClient::new(aws_sdk_sqs::Client::new(&config)
                            .send_message()
                            .queue_url(v));

                        queues.insert(k.replace(\"KINETICS_QUEUE_\", \"\"), queue_client);
                    }}
                }}

                println!(\"Serving requests\");

                run(service_fn(|_event: LambdaEvent<EventBridgeEvent<serde_json::Value>>| async {{
                    match {rust_function_name}(&secrets, &queues).await {{
                        Ok(()) => Ok(()),
                        Err(err) => {{
                            eprintln!(\"Error occurred while handling request: {{:?}}\", err);
                            Err(err)
                        }}
                    }}
                }}))
                .await
            }}\n\n"
        )
    }
}
