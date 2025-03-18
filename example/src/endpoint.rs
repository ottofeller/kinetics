use aws_sdk_dynamodb::types::AttributeValue::S;
use aws_sdk_dynamodb::Client;
use kinetics_macro::endpoint;
use lambda_http::{Body, Error, Request, RequestExt, Response};
use std::collections::HashMap;

#[endpoint(
    url_path = "/some",
    environment = {"DEFAULT_NAME": "John"},
)]
pub async fn endpoint(
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
