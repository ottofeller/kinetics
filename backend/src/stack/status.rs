use crate::auth::session::Session;
use crate::env::env;
use crate::json;
use crate::stack::Stack;
use aws_config::BehaviorVersion;
use aws_sdk_cloudformation::types::StackEvent;
use aws_sdk_sqs::operation::send_message::builders::SendMessageFluentBuilder;
use eyre::Result;
use kinetics_macro::endpoint;
use lambda_http::{Body, Error, Request, Response};
use serde_json::json;
use std::collections::HashMap;

#[derive(serde::Deserialize, Debug)]
pub struct JsonBody {
    pub stack_name: String,
}

/*
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Action": [
                "logs:CreateLogGroup",
                "logs:CreateLogStream",
                "logs:PutLogEvents",
                "cloudformation:DescribeStackEvents"
            ],
            "Resource": "*",
            "Effect": "Allow"
        }
    ]
}
*/
#[endpoint(url_path = "/stack/status", environment = {
    "TABLE_NAME": "kinetics",
})]
pub async fn status(
    event: Request,
    _secrets: &HashMap<String, String>,
    _queues: &HashMap<String, SendMessageFluentBuilder>,
) -> Result<Response<Body>, Error> {
    fn map_stack_event(event: &StackEvent) -> serde_json::Value {
        json!({
            "Status": event.resource_status().unwrap().as_str(),
            "Reason": event.resource_status_reason(),
            "ResourceType": event.resource_type().unwrap(),
            "Timestamp": event.timestamp().unwrap().to_string()
        })
    }

    let session = Session::new(&event, &env("TABLE_NAME")?).await?;

    if !session.is_valid() {
        eprintln!("Not authorized");
        return json::response(json!({"error": "Unauthorized"}), Some(403));
    }

    let body = json::body::<JsonBody>(event)?;

    let config = aws_config::defaults(BehaviorVersion::v2025_01_17())
        .load()
        .await;

    let client = aws_sdk_cloudformation::Client::new(&config);
    let stack = Stack::new(&session.username(true), &body.stack_name);
    let mut next_token = None;
    let mut all_events = Vec::new();
    let mut start_event = None;
    let mut end_event_success = None;
    let mut end_event_failure = None;

    loop {
        let mut req = client
            .describe_stack_events()
            .stack_name(stack.clone().name);

        if let Some(token) = next_token {
            req = req.next_token(token);
        }

        let response = req.send().await?;
        let events = response.stack_events();

        for event in events {
            let is_stack_event = event
                .resource_type()
                .unwrap()
                .eq("AWS::CloudFormation::Stack");

            let json = map_stack_event(&event);

            if matches!(
                event.resource_status_reason().unwrap_or_default(),
                "User Initiated"
            ) && start_event.is_none()
                && is_stack_event
            {
                start_event = Some(json.clone());
            }

            // Once failure or success event found no need in searching for other failure or success event
            if matches!(
                event.resource_status().unwrap().as_str(),
                "UPDATE_ROLLBACK_COMPLETE"
                    | "UPDATE_ROLLBACK_FAILED"
                    | "CREATE_FAILED"
                    | "UPDATE_FAILED"
                    | "DELETE_FAILED"
            ) && end_event_failure.is_none()
                && end_event_success.is_none()
                && is_stack_event
            {
                end_event_failure = Some(json.clone());
            }

            if matches!(
                event.resource_status().unwrap().as_str(),
                "UPDATE_COMPLETE" | "CREATE_COMPLETE" | "DELETE_COMPLETE"
            ) && end_event_success.is_none()
                && end_event_failure.is_none()
                && is_stack_event
            {
                end_event_success = Some(json.clone());
            }

            if start_event.is_none() {
                all_events.push(json);
            }
        }

        next_token = response.next_token().map(|s| s.to_string());

        if next_token.is_none() || start_event.is_some() {
            break;
        }
    }

    if end_event_success.is_none() && end_event_failure.is_none() {
        return json::response(json!({"status": "IN_PROGRESS"}), None);
    }

    if end_event_success.is_some() {
        return json::response(json!({"status": "COMPLETE"}), None);
    }

    // Find all failed events and accumulate the error response
    let mut errors = vec![];

    for event in all_events.iter() {
        if let Some(status) = event.get("Status").and_then(|s| s.as_str()) {
            if status.contains("FAILED") {
                if let Some(resource_type) = event.get("ResourceType").and_then(|t| t.as_str()) {
                    if resource_type != "AWS::CloudFormation::Stack" {
                        errors.push(event.get("Reason"));
                    }
                }
            }
        }
    }

    return json::response(json!({"status": "FAILED", "errors": errors}), None);
}
