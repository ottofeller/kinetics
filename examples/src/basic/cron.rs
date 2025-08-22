use kinetics::macros::cron;
use std::collections::HashMap;

/// A regular cron job which prints out every hour
///
/// Test locally with the following command:
/// kinetics invoke BasicCronCron
#[cron(schedule = "rate(1 hour)")]
pub async fn cron(_secrets: &HashMap<String, String>) -> Result<(), Box<dyn std::error::Error>> {
    println!("Started cron job");
    Ok(())
}
