use aws_lambda_events::{lambda_function_urls::LambdaFunctionUrlRequest, sqs::SqsEvent, sqs::SqsBatchResponse};
use lambda_http::{Body, Response};
use lambda_runtime::{LambdaEvent, Error};
use skymacro::{endpoint, worker};

#[endpoint(name = "Some", url_path = "/some")]
pub async fn some_endpoint(
    event: LambdaEvent<LambdaFunctionUrlRequest>,
) -> Result<Response<Body>, Error> {
    let default = String::from("Nobody");

    let who = event
        .payload
        .query_string_parameters
        .get("name")
        .unwrap_or(&default);

    let message = format!("Hello {who}, this is an AWS Lambda HTTP request");

    // Return something that implements IntoResponse.
    // It will be serialized to the right response event automatically by the runtime
    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body(message.into())
        .map_err(Box::new)?;

    Ok(resp)
}

#[worker(name = "aworker", concurrency = 3, fifo = true)]
pub async fn some_worker(_event: LambdaEvent<SqsEvent>) -> Result<SqsBatchResponse, Error> {
    let sqs_batch_response = SqsBatchResponse::default();
    Ok(sqs_batch_response)
}
