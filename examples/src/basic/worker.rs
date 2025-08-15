use aws_lambda_events::sqs::{BatchItemFailure, SqsBatchResponse, SqsEvent};
use kinetics::tools::queue::Client as QueueClient;
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
) -> Result<SqsBatchResponse, Error> {
    let mut sqs_batch_response = SqsBatchResponse::default();

    // Always return the first record from the input batch in batch item failure, just for example
    // Doing so will force the worker to process the item again on the next iteration
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

    // Optional: Return a batch item failure to retry the message
    sqs_batch_response
        .batch_item_failures
        .push(BatchItemFailure {
            item_identifier: record.message_id.clone().unwrap_or_default(),
        });

    Ok(sqs_batch_response)
}
