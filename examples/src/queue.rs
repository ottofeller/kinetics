use aws_sdk_sqs::operation::send_message::builders::SendMessageFluentBuilder;
use kinetics::macros::endpoint;
use kinetics::tools::queue::Client;
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
    queues: &HashMap<String, SendMessageFluentBuilder>,
) -> Result<Response<Body>, Error> {
    let client = Client::new(queues["example"].clone());
    client.send("Test message").await?;

    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body(json!({"success": true}).to_string().into())
        .map_err(Box::new)?;

    Ok(resp)
}
