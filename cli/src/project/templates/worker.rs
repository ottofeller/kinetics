pub fn worker(import_statement: &str, rust_function_name: &str, is_local: bool) -> String {
    if is_local {
        format!(
            "{import_statement}
            use aws_lambda_events::sqs::{{SqsEvent, SqsMessage}};
            use kinetics_lib::tools::{{queue::Record as QueueRecord, config::Config as KineticsConfig}};
            #[tokio::main]
            async fn main() -> Result<(), tower::BoxError> {{
                let user_function = {rust_function_name};
                let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                let kinetics_config = KineticsConfig::new(&config, None).await?;
                let mut secrets = std::collections::HashMap::new();

                for (k, v) in std::env::vars() {{
                    if k.starts_with(\"KINETICS_SECRET_\") {{
                        let key = k.replace(\"KINETICS_SECRET_\", \"\");
                        secrets.insert(key, v);
                    }}
                }}

                let payload_path = std::env::args_os().nth(1).ok_or_else(|| {{
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        \"Payload file path is required as the first argument\",
                    )
                }})?;

                let payload = std::fs::read_to_string(&payload_path).map_err(|err| {{
                    std::io::Error::new(
                        err.kind(),
                        format!(
                            \"Failed to read payload file '{{}}': {{err}}\",
                            std::path::Path::new(&payload_path).display(),
                        ),
                    )
                }})?;

                let payloads: Vec<serde_json::Value> = serde_json::from_str(&payload).map_err(|err| {{
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            \"Failed to parse worker payload file '{{}}' as a JSON array: {{err}}\",
                            std::path::Path::new(&payload_path).display(),
                        ),
                    )
                }})?;

                let mut records = Vec::with_capacity(payloads.len());
                for (index, payload) in payloads.into_iter().enumerate() {{
                    let mut sqs_message = SqsMessage::default();
                    sqs_message.message_id = Some(format!(\"test-{{}}\", index + 1));
                    sqs_message.body = Some(serde_json::to_string(&payload).map_err(|err| {{
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!(\"Failed to serialize worker payload item {{}}: {{err}}\", index + 1),
                        )
                    }})?);
                    records.push(sqs_message);
                }}

                let mut sqs_event = SqsEvent::default();
                sqs_event.records = records;

                // Convert SqsEvent to LambdaEvent<SqsEvent>
                let context = lambda_runtime::Context::default();
                let event = lambda_runtime::LambdaEvent::new(sqs_event, context);

                if let Err(err) = user_function(QueueRecord::from_sqsevent(event)?, &secrets, &kinetics_config).await {{
                    eprintln!(\"Request failed: {{:?}}\", err);
                }}

                Ok(())
            }}"
        )
    } else {
        format!(
            "{import_statement}
            use lambda_runtime::{{Error, run, service_fn}};\n\
            use kinetics_lib::tools::{{queue::Record as QueueRecord, config::Config as KineticsConfig}};
            #[tokio::main]\n\
            async fn main() -> Result<(), Error> {{\n\
                let user_function = {rust_function_name};
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

                let kinetics_config = KineticsConfig::new(&config, None).await?;
                println!(\"Serving requests\");

                run(service_fn(|event| async {{
                    match user_function(QueueRecord::from_sqsevent(event)?, &secrets, &kinetics_config).await {{
                        Ok(response) => Ok(response.collect()),
                        Err(err) => {{
                            eprintln!(\"Error occurred while handling request: {{:?}}\", err);
                            Err(err)
                        }}
                    }}
                }})).await
            }}
"
        )
    }
}
