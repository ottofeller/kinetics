use kinetics_macro::cron;
use std::collections::HashMap;

#[cron(schedule = "rate(1 hour)")]
pub async fn cron(_secrets: &HashMap<String, String>) -> Result<(), Box<dyn std::error::Error>> {
    println!("Started cron job");
    Ok(())
}
