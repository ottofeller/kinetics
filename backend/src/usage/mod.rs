use crate::{auth::session::Session, env::env, json::response as json_response};
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::types::AttributeValue;
use kinetics_macro::endpoint;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use std::collections::HashMap;

#[endpoint(url_path = "/usage/increment", environment = {
    "TABLE_NAME": "kinetics",
    "USAGE_LIMIT": "100000",
    "DANGER_DISABLE_AUTH": "false"
})]
pub async fn increment(
    event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let session = Session::new(&event, &env("TABLE_NAME")?).await;

    if env("DANGER_DISABLE_AUTH")? == "false" && !session.as_ref().unwrap().is_valid() {
        eprintln!("Not authorized");
        return json_response(json!({"error": "Unauthorized"}), None);
    }

    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let dynamodb_client = aws_sdk_dynamodb::Client::new(&config);
    let id = AttributeValue::S(format!("usage#{}", session?.username(false)));
    let name = "calls";

    let get_item = dynamodb_client
        .get_item()
        .table_name(env("TABLE_NAME")?)
        .key("id", id.clone())
        .send()
        .await?;

    let current_count = match get_item.item {
        Some(item) => item
            .get(name)
            .unwrap_or(&AttributeValue::N("0".into()))
            .as_n()
            .unwrap_or(&String::from("0"))
            .parse::<i32>()
            .unwrap_or(0),
        None => 0,
    };

    if current_count >= 100000 {
        return json_response(json!({"error": "Endpoint count exceeded limit"}), Some(429));
    }

    let new_count = current_count + 1;

    dynamodb_client
        .update_item()
        .table_name(env("TABLE_NAME")?)
        .key("id", id)
        .update_expression("SET #name = :val")
        .expression_attribute_names("#name", name)
        .expression_attribute_values(":val", AttributeValue::N(new_count.to_string()))
        .send()
        .await?;

    json_response(json!({"success": true, "count": new_count}), None)
}
