use ::serde::{Deserialize, Serialize};
use aws_lambda_events::{
    lambda_function_urls::LambdaFunctionUrlRequest,
    sqs::{SqsBatchResponse, SqsEvent},
};
use lambda_runtime::{Error, LambdaEvent};
use skymacro::{endpoint, worker};

#[derive(Deserialize, Serialize)]
pub struct Response {
    pub status_code: i32,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[endpoint(name = "Some", url_path = "/some")]
pub async fn some_endpoint(
    event: LambdaEvent<LambdaFunctionUrlRequest>,
) -> Result<Response, Error> {
    let default = String::from("Nobody");

    let who = event
        .payload
        .query_string_parameters
        .get("name")
        .unwrap_or(&default);

    Ok(Response {
        status_code: 200,
        headers: vec![],
        body: format!("Hello {who}, this is an AWS Lambda HTTP request").into(),
    })
}

#[worker(name = "aworker", concurrency = 3, fifo = true)]
pub async fn some_worker(_event: LambdaEvent<SqsEvent>) -> Result<SqsBatchResponse, Error> {
    let sqs_batch_response = SqsBatchResponse::default();
    Ok(sqs_batch_response)
}
