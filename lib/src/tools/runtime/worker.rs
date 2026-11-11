/// Generate a worker entry point using this runtime version.
#[macro_export]
macro_rules! worker_main {
    (local, $handler:path) => {
        use aws_lambda_events::sqs::{SqsEvent, SqsMessage};
        use $crate::tools::runtime::dependencies::{
            aws_config, aws_lambda_events, lambda_runtime, serde_json, tokio, tower,
        };
        use $crate::tools::{config::Config as KineticsConfig, queue::Record as QueueRecord};
        #[tokio::main]
        async fn main() -> Result<(), tower::BoxError> {
            let user_function = $handler;
            let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
            let kinetics_config = KineticsConfig::new(&config, None).await?;
            let mut secrets = std::collections::HashMap::new();

            for (k, v) in std::env::vars() {
                if k.starts_with("KINETICS_SECRET_") {
                    let key = k.replace("KINETICS_SECRET_", "");
                    secrets.insert(key, v);
                }
            }

            let payload_path = std::env::args_os().nth(1).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Payload file path is required as the first argument",
                )
            })?;

            let payload = std::fs::read_to_string(&payload_path).map_err(|err| {
                std::io::Error::new(
                    err.kind(),
                    format!(
                        "Failed to read payload file '{}': {err}",
                        std::path::Path::new(&payload_path).display(),
                    ),
                )
            })?;

            let payloads: Vec<serde_json::Value> =
                serde_json::from_str(&payload).map_err(|err| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "Failed to parse worker payload file '{}' as a JSON array: {err}",
                            std::path::Path::new(&payload_path).display(),
                        ),
                    )
                })?;

            let mut records = Vec::with_capacity(payloads.len());
            for (index, payload) in payloads.into_iter().enumerate() {
                let mut sqs_message = SqsMessage::default();
                sqs_message.message_id = Some(format!("test-{}", index + 1));

                sqs_message.body = Some(match &payload {
                    serde_json::Value::String(body) => body.clone(),
                    other => serde_json::to_string(other).map_err(|err| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!(
                                "Failed to serialize worker payload item {}: {err}",
                                index + 1
                            ),
                        )
                    })?,
                });
                records.push(sqs_message);
            }

            let mut sqs_event = SqsEvent::default();
            sqs_event.records = records;

            // Convert SqsEvent to LambdaEvent<SqsEvent>
            let context = lambda_runtime::Context::default();
            let event = lambda_runtime::LambdaEvent::new(sqs_event, context);

            match user_function(
                QueueRecord::from_sqsevent(event)?,
                &secrets,
                &kinetics_config,
            )
            .await
            {
                Ok(retries) => {
                    let retries = retries.collect();

                    let failures = if retries.batch_item_failures.is_empty() {
                        "none".into()
                    } else {
                        serde_json::to_string(&retries.batch_item_failures).unwrap_or_default()
                    };
                    println!("Batch item failures: {failures}");
                }
                Err(err) => {
                    return Err(format!("Request failed: {err:?}").into());
                }
            }

            Ok(())
        }
    };
    (remote, $handler:path) => {
        use lambda_runtime::{run, service_fn, Error};
        use $crate::tools::runtime::dependencies::{aws_config, lambda_runtime, tokio};
        use $crate::tools::{config::Config as KineticsConfig, queue::Record as QueueRecord};
        #[tokio::main]
        async fn main() -> Result<(), Error> {
            let user_function = $handler;
            let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
            let runtime_config = $crate::tools::runtime::load(&config).await?;
            println!("Provisioning secrets");
            let (secrets, kinetics_config) = tokio::try_join!(
                async {
                    $crate::tools::secrets::load(&config, runtime_config)
                        .await
                        .map_err(Error::from)
                },
                KineticsConfig::new(&config, None),
            )?;
            println!("Serving requests");

            run(service_fn(|event| async {
                match user_function(
                    QueueRecord::from_sqsevent(event)?,
                    &secrets,
                    &kinetics_config,
                )
                .await
                {
                    Ok(response) => Ok(response.collect()),
                    Err(err) => {
                        eprintln!("Error occurred while handling request: {:?}", err);
                        Err(err)
                    }
                }
            }))
            .await
        }
    };
}
