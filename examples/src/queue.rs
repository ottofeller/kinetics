use kinetics::macros::endpoint;
use kinetics::tools::queue::Client as QueueClient;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use std::collections::HashMap;

/// Send a message to the queue
///
/// Must be processed by worker with #[worker(queue_alias = "example")] macro.
/// Can't be tested locally, as it requires access to the queue. Deploy with this command:
/// kinetics deploy
#[endpoint(url_path = "/queue")]
pub async fn queue(
    _event: Request,
    _secrets: &HashMap<String, String>,
    queues: &HashMap<String, QueueClient>,
) -> Result<Response<Body>, Error> {
    let client = match queues.get("example") {
        Some(client) => client,
        None => return Err(Error::from("Queue not found")),
    };

    client.send("Test message").await?;

    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body(json!({"success": true}).to_string().into())
        .map_err(Box::new)?;

    Ok(resp)
}
