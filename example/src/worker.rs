use aws_lambda_events::sqs::{SqsBatchResponse, SqsEvent};
use aws_sdk_sqs::operation::send_message::builders::SendMessageFluentBuilder;
use kinetics_macro::worker;
use lambda_http::Error;
use lambda_runtime::LambdaEvent;
use std::collections::HashMap;

#[worker(
    concurrency = 3,
    fifo = true,
    environment = {"CURRENCY": "USD"},
    queue_alias = "example_worker",
)]
pub async fn worker(
    event: LambdaEvent<SqsEvent>,
    secrets: &HashMap<String, String>,
    _queues: &HashMap<String, SendMessageFluentBuilder>,
) -> Result<SqsBatchResponse, Error> {
    println!(
        "Environment: {:?}",
        std::env::vars().collect::<HashMap<_, _>>()
    );
    println!("Event: {event:?}");
    println!("Secrets: {secrets:?}");
    let sqs_batch_response = SqsBatchResponse::default();
    Ok(sqs_batch_response)
}
