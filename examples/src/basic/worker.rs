use aws_lambda_events::sqs::SqsEvent;
use kinetics::tools::queue::{Client as QueueClient, Retries as QueueRetries};
use kinetics_macro::worker;
use lambda_runtime::{Error, LambdaEvent};
use std::collections::HashMap;

/// A queue worker
///
/// Always returns the first record as failed to process. It will then be retried.
/// Test locally with the following command:
/// kinetics invoke BasicWorkerWorker --payload '{"name": "John"}'
#[worker(fifo = true, queue_alias = "example")]
pub async fn worker(
    event: LambdaEvent<SqsEvent>,
    _secrets: &HashMap<String, String>,
    _queues: &HashMap<String, QueueClient>,
) -> Result<QueueRetries, Error> {
    let mut retries = QueueRetries::new();

    let record = match event.payload.records.first() {
        Some(record) => record,
        None => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No records found",
            )))
        }
    };

    let body = serde_json::Value::from(record.body.clone().unwrap());
    println!("Got body: {body:?}");

    // Optionally return the first record from the input batch in retries, just for example
    // Doing so will force the worker to process the item again on the next iteration
    retries.add(&record.message_id.clone().unwrap_or_default());

    Ok(retries)
}
