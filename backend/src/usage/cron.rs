use crate::{env::env, user::UserBuilder};
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::types::AttributeValue::S;
use aws_sdk_dynamodb::Client;
use aws_sdk_sqs::operation::send_message::builders::SendMessageFluentBuilder;
use eyre::Context;
use kinetics_macro::cron;
use std::collections::HashMap;

/// Bill users for usage
///
/// Also block free users if they go over free tier.
#[cron(schedule = "rate(1 minute)", environment = {
    "TABLE_NAME": "kinetics",
    "INVOCATIONS_LIMIT": "500",
    "DO_NOT_THROTTLE_USER": "alex@ottofeller.com"
})]
pub async fn cron(
    _secrets: &HashMap<String, String>,
    _queues: &HashMap<String, SendMessageFluentBuilder>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let db_client = Client::new(&config);
    let table = env("TABLE_NAME")?;
    let limit = env("INVOCATIONS_LIMIT")?.parse::<u16>()?;

    let request = db_client
        .scan()
        .table_name(&table)
        .filter_expression("begins_with(id, :prefix)")
        .expression_attribute_values(":prefix", S("email#".to_string()));

    let builder = UserBuilder::new(&db_client, &table);

    for item in request.send().await?.items().iter() {
        let email = item
            .get("id")
            .unwrap()
            .as_s()
            .unwrap()
            .to_string()
            .replace("email#", "");

        if email == env("DO_NOT_THROTTLE_USER")? {
            continue;
        }

        let mut user = builder
            .by_email(&email)
            .await
            .wrap_err("Failed to get user by email")?;

        println!(
            "Number of invocations for {} in this month: {:?}",
            user.email,
            user.invocations("month")
                .await
                .wrap_err("Failed to count invoications for user")?
        );

        user.throttle(user.invocations("month").await? >= limit)
            .await?;
    }

    Ok(())
}
