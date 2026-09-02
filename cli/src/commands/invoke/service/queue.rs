use crate::error::Error;
use aws_config::BehaviorVersion;
use std::time::Duration;

const DOCKER_COMPOSE_SNIPPET: &str = r#"
local-sqs:
    image: "vsouza/sqs-local:1.5.7"
    ports:
        - "{{PORT}}"
"#;

#[derive(Clone)]
pub struct LocalQueue {
    name: String,
    port: u16,
}

impl LocalQueue {
    pub fn new() -> Self {
        Self {
            name: "local-queue".to_string(),
            port: 9324,
        }
    }

    /// A queue with an explicit name
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_string(),
            port: 9324,
        }
    }

    pub fn docker_compose_snippet(&self) -> String {
        DOCKER_COMPOSE_SNIPPET.replace(
            "{{PORT}}",
            format!("{port}:{port}", port = self.port).as_str(),
        )
    }

    pub async fn provision(&self) -> eyre::Result<()> {
        let client = self.client().await;

        // Retry parameters
        let max_retries = 5;
        let retry_delay_ms = 1000;

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

                    tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;
                }
            }
        }

        log::info!("Queue '{}' created successfully.", self.name);
        Ok(())
    }

    /// Build an SQS client configured for the local queue.
    pub async fn client(&self) -> aws_sdk_sqs::Client {
        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(self.endpoint_url())
            .region("us-east-1")
            .credentials_provider(aws_sdk_sqs::config::Credentials::new(
                "key", "secret", None, None, "provider",
            ))
            .load()
            .await;

        aws_sdk_sqs::Client::new(&config)
    }

    /// The fixed account id used by the local SQS emulator.
    pub fn account_id(&self) -> &str {
        "000000000000"
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn endpoint_url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }
}
