use aws_sdk_dynamodb::types::AttributeValue::S;
use aws_sdk_dynamodb::Client;
use aws_sdk_sqs::operation::send_message::builders::SendMessageFluentBuilder;
use kinetics_macro::endpoint;
use lambda_http::{Body, Error, Request, RequestExt, Response};
use std::collections::HashMap;

#[endpoint(
    url_path = "/endpoint",
    environment = {"DEFAULT_NAME": "John"},
)]
pub async fn endpoint(
    event: Request,
    secrets: &HashMap<String, String>,
    queues: &HashMap<String, SendMessageFluentBuilder>,
) -> Result<Response<Body>, Error> {
    let default = String::from("Nobody");
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = Client::new(&config);
    println!(
        "Environment: {:?}",
        std::env::vars().collect::<HashMap<_, _>>()
    );
    println!("Secrets: {secrets:?}");
    println!("Queues: {queues:?}");

    queues["example_worker"]
        .clone()
        .message_body("Test message")
        .send()
        .await?;

    queues["example_worker"]
        .clone()
        .message_body("Another test message")
        .send()
        .await?;

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
