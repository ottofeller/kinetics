use http::{Request, Response, StatusCode};
use kinetics_lib::macros::{dlq_worker, endpoint, worker};
use kinetics_lib::tools::config::Config as KineticsConfig;
use kinetics_lib::tools::queue::{
    Client as QueueClient, Record as QueueRecord, Retries as QueueRetries,
};
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

/// Send a message to the queue
#[endpoint(url_path = "/rejecting-queue")]
pub async fn rejecting_queue(
    _event: Request<Vec<u8>>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<String>, BoxError> {
    let client = QueueClient::from_worker(reject_records).await?;
    client.send("Test message").await?;

    let resp = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html")
        .body("Ok".to_string())
        .map_err(Box::new)?;
    Ok(resp)
}

/// A queue worker that rejects every message
///
/// Rejected records are returned for retry
/// and once the queue's `max_receive_count` attempts are exhausted,
/// SQS redrives the message to the dead-letter queue.
#[worker(dlq = log_rejected, max_receive_count = 3)]
pub async fn reject_records(
    records: Vec<QueueRecord>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<QueueRetries, BoxError> {
    let mut retries = QueueRetries::new();

    for record in &records {
        println!("Rejecting: {:?}", record.body);
        retries.add(&record.message_id.clone().unwrap_or_default());
    }

    Ok(retries)
}

/// A DLQ recovery handler
///
/// Consumes the dead-letter queue of `reject_records`
/// and logs every message the worker rejected.
#[dlq_worker()]
pub async fn log_rejected(
    records: Vec<QueueRecord>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<QueueRetries, BoxError> {
    for record in &records {
        println!("DLQ: {}", record.body.clone().unwrap_or_default());
    }

    Ok(QueueRetries::new())
}
