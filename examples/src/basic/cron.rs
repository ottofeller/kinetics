use aws_sdk_sqs::operation::send_message::builders::SendMessageFluentBuilder;
use kinetics::cron;
use std::collections::HashMap;

/// A regular cron job which prints out every hour
///
/// Test locally with the following command:
/// kinetics invoke CronCron
#[cron(schedule = "rate(1 hour)")]
pub async fn cron(
    _secrets: &HashMap<String, String>,
    _queues: &HashMap<String, SendMessageFluentBuilder>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Started cron job");
    Ok(())
}
