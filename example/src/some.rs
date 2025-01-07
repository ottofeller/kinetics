use aws_lambda_events::sqs::{SqsBatchResponse, SqsEvent};
use lambda_http::{Body, Error, Request, RequestExt, Response};
use lambda_runtime::LambdaEvent;
use skymacro::{endpoint, worker};

#[endpoint(name = "Some", url_path = "/some")]
pub async fn some_endpoint(event: Request) -> Result<Response<Body>, Error> {
    let default = String::from("Nobody");
    use aws_sdk_dynamodb::types::AttributeValue::S;
    use aws_sdk_dynamodb::Client;
    use std::collections::HashMap;
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = Client::new(&config);

    client
        .put_item()
        .table_name("users")
        .set_item(Some(HashMap::from([
            ("id".to_string(), S("user123".to_string())),

            (
                "name".to_string(),
                S(event
                    .query_string_parameters()
                    .first("who")
                    .unwrap_or(&default)
                    .to_string()),
            ),
        ])))
        .send()
        .await?;

    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body("Hello AWS Lambda HTTP request".into())
        .map_err(Box::new)?;
    Ok(resp)
}

#[worker(name = "aworker", concurrency = 3, fifo = true)]
pub async fn some_worker(_event: LambdaEvent<SqsEvent>) -> Result<SqsBatchResponse, Error> {
    let sqs_batch_response = SqsBatchResponse::default();
    Ok(sqs_batch_response)
}
