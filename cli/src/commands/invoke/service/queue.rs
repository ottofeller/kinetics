use crate::error::Error;
use aws_config::BehaviorVersion;
use std::time::Duration;

const DOCKER_COMPOSE_SNIPPET: &str = r#"
local-sqs:
    image: "vsouza/sqs-local:1.5.7"
    ports:
        - "9324:9324"
"#;

pub struct LocalQueue {
    name: String,
}

impl LocalQueue {
    pub fn new() -> Self {
        Self {
            name: "local-queue".to_string(),
        }
    }

    pub fn docker_compose_snippet(&self) -> &str {
        DOCKER_COMPOSE_SNIPPET
    }

    pub async fn provision(&self) -> eyre::Result<()> {
        // Configure AWS client
        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url("http://localhost:9324")
            .region("us-east-1")
            .credentials_provider(aws_sdk_sqs::config::Credentials::new(
                "key", "secret", None, None, "provider",
            ))
            .load()
            .await;

        let client = aws_sdk_sqs::Client::new(&config);

        // Retry parameters
        let max_retries: u32 = 3;
        let initial_delay_ms: u64 = 200;

        // Wait for SQS to be ready and attempt to create the queue with retries
        for attempt in 1..=max_retries {
            let result = client.create_queue().queue_name(&self.name).send().await;

            match result {
                Ok(_) => return Ok(()),
                Err(_) => {
                    if attempt == max_retries {
                        return Err(Error::new(
                            "Failed to create queue",
                            Some("Make sure the docker container is running and available at http://localhost:9324"),
                        ).into());
                    }

                    let multiplier = 2u64.saturating_pow(attempt.saturating_sub(1));
                    let delay_duration = initial_delay_ms.saturating_mul(multiplier);
                    tokio::time::sleep(Duration::from_millis(delay_duration)).await;
                }
            }
        }

        log::info!("Queue '{}' created successfully.", self.name);
        Ok(())
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn endpoint_url(&self) -> String {
        // Keep in mind that the port is hardcoded in DOCKER_COMPOSE_SNIPPET
        "http://localhost:9324".to_string()
    }
}
