use kinetics::macros::worker;
use kinetics::tools::queue::{Record as QueueRecord, Retries as QueueRetries};
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

/// A queue worker
///
/// Always returns the first record as failed to process. It will then be retried.
/// Test locally with the following command:
/// kinetics invoke BasicWorkerWorker --payload '{"name": "John"}'
#[worker(fifo = true)]
pub async fn worker(
    records: Vec<QueueRecord>,
    _secrets: &HashMap<String, String>,
) -> Result<QueueRetries, BoxError> {
    let mut retries = QueueRetries::new();

    let record = match records.first() {
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
