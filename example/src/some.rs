use aws_lambda_events::sqs::{SqsBatchResponse, SqsEvent};
use aws_sdk_dynamodb::types::AttributeValue::S;
use aws_sdk_dynamodb::Client;
use lambda_http::{Body, Error, Request, RequestExt, Response};
use lambda_runtime::LambdaEvent;
use skymacro::{endpoint, worker};
use std::collections::HashMap;

#[endpoint(
    name = "Some",
    url_path = "/some",
    environment = {"DEFAULT_NAME": "John"},
)]
pub async fn some_endpoint(
    event: Request,
    secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let default = String::from("Nobody");
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = Client::new(&config);
    println!(
        "Environment: {:?}",
        std::env::vars().collect::<HashMap<_, _>>()
    );
    println!("Secrets: {secrets:?}");

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

#[worker(
    name = "aworker",
    concurrency = 3,
    fifo = true,
    environment = {"CURRENCY": "USD"},
)]
pub async fn some_worker(
    _event: LambdaEvent<SqsEvent>,
    secrets: &HashMap<String, String>,
) -> Result<SqsBatchResponse, Error> {
    println!(
        "Environment: {:?}",
        std::env::vars().collect::<HashMap<_, _>>()
    );
    println!("Secrets: {secrets:?}");
    let sqs_batch_response = SqsBatchResponse::default();
    Ok(sqs_batch_response)
}
