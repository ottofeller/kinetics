/// Generate a worker entry point using this runtime version.
#[macro_export]
macro_rules! worker_main {
    (local, $handler:path) => {
        use aws_lambda_events::sqs::{SqsEvent, SqsMessage};
        use $crate::tools::queue::Record as QueueRecord;
        use $crate::tools::runtime::dependencies::{
            aws_lambda_events, lambda_runtime, serde_json, tokio, tower,
        };
        use $crate::tools::runtime::{Environment, Runtime};
        #[tokio::main]
        async fn main() -> Result<(), tower::BoxError> {
            let Runtime {
                secrets,
                config: kinetics_config,
            } = Runtime::load(Environment::Local, None).await?;

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

            match $handler(
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
        use lambda_runtime::{Error, run, service_fn};
        use $crate::tools::queue::Record as QueueRecord;
        use $crate::tools::runtime::dependencies::{lambda_runtime, tokio};
        use $crate::tools::runtime::{Environment, Runtime};
        #[tokio::main]
        async fn main() -> Result<(), Error> {
            println!("Initializing runtime");
            let Runtime {
                secrets,
                config: kinetics_config,
            } = Runtime::load(Environment::Lambda, None).await?;
            println!("Serving requests");
            run(service_fn(|event| async {
                match $handler(
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
