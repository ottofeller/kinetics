use kinetics_lib::macros::worker;
use kinetics_lib::tools::config::Config as KineticsConfig;
use kinetics_lib::tools::queue::{Record as QueueRecord, Retries as QueueRetries};
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

/// A queue worker
///
/// Processes and prints every record in the input batch.
/// Always returns only the first record as failed to process. It will then be retried.
///
/// Test locally with the following command:
/// kinetics invoke BasicWorkerWorker --payload '[{"name": "John"}, {"name": "Jane"}]'
#[worker(fifo = true, batch_size = 2)]
pub async fn worker(
    records: Vec<QueueRecord>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<QueueRetries, BoxError> {
    let mut retries = QueueRetries::new();
    println!("Got batch of {} records", records.len());

    let first_record = match records.first() {
        Some(record) => record,
        None => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No records found",
            )))
        }
    };

    // Process every record in the input batch.
    for record in &records {
        let body = serde_json::Value::from(record.body.clone().unwrap());
        println!("Got message: {body:?}");
    }

    // Optionally return only the first record from the input batch in retries, just for example
    // Doing so will force the worker to process the item again on the next iteration
    retries.add(&first_record.message_id.clone().unwrap_or_default());

    Ok(retries)
}
