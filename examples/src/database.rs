use aws_sdk_dynamodb::types::AttributeValue::S;
use aws_sdk_dynamodb::Client;
use kinetics::macros::endpoint;
use kinetics::tools::queue::Client as QueueClient;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use std::collections::HashMap;

/// Simply put an item into DB and then retrieve it
///
/// Test locally with the following command:
/// kinetics invoke DatabaseDatabase --table mytable
#[endpoint(
    url_path = "/database",
    environment = {"TABLE_NAME": "mytable"},
)]
pub async fn database(
    _event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = Client::new(&config);
    let env = std::env::vars().collect::<HashMap<_, _>>();
    let table_name = env.get("TABLE_NAME").unwrap();
    let id = String::from("user123");

    client
        .put_item()
        .table_name(table_name)
        .set_item(Some(HashMap::from([
            ("id".to_string(), S(id.clone())),
            ("name".to_string(), S("A name".to_string())),
        ])))
        .send()
        .await?;

    let item = client
        .get_item()
        .table_name(table_name)
        .key("id", S(id))
        .send()
        .await?
        .item
        .unwrap();

    let name = item.get("name").unwrap().as_s().unwrap();

    let resp = Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(json!({"success": true, name: name}).to_string().into())
        .map_err(Box::new)?;

    Ok(resp)
}
