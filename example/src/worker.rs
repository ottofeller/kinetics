use aws_lambda_events::sqs::{SqsBatchResponse, SqsEvent};
use kinetics_macro::worker;
use lambda_http::Error;
use lambda_runtime::LambdaEvent;
use std::collections::HashMap;

#[worker(
    concurrency = 3,
    fifo = true,
    environment = {"CURRENCY": "USD"},
)]
pub async fn some_worker(
    event: LambdaEvent<SqsEvent>,
    secrets: &HashMap<String, String>,
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
