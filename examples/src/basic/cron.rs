use kinetics::macros::cron;
use kinetics::tools::config::Config as KineticsConfig;
use std::collections::HashMap;
use tower::BoxError;

/// A regular cron job which prints out every hour
///
/// Test locally with the following command:
/// kinetics invoke BasicCronCron
#[cron(schedule = "rate(1 hour)")]
pub async fn cron(
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<(), BoxError> {
    println!("Started cron job");
    Ok(())
}
