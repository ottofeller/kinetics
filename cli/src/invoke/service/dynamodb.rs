use crate::error::Error;
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, KeySchemaElement, KeyType, ScalarAttributeType,
};

const DOCKER_COMPOSE_SNIPPET: &str = r#"
local-dynamodb:
    command: "-jar DynamoDBLocal.jar -sharedDb -dbPath ./data"
    image: "amazon/dynamodb-local:latest"
    ports:
        - "8000:8000"
    volumes:
        - "/tmp/dynamodb:/home/dynamodblocal/data"
    working_dir: /home/dynamodblocal
"#;

/// Manage local DynamoDB (container and table)
pub struct LocalDynamoDB {
    table: String,
}

impl LocalDynamoDB {
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
        }
    }

    pub fn docker_compose_snippet(&self) -> &str {
        DOCKER_COMPOSE_SNIPPET
    }

    /// Provision table with retry mechanism for handling connection issues
    pub async fn provision(&self) -> eyre::Result<()> {
        // Configure AWS client
        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url("http://localhost:8000")
            .region("us-east-1")
            .credentials_provider(aws_sdk_dynamodb::config::Credentials::new(
                "key", "secret", None, None, "provider",
            ))
            .load()
            .await;

        let client = aws_sdk_dynamodb::Client::new(&config);

        // Retry parameters
        let max_retries = 5;
        let initial_delay_ms = 500;

        // Wait for DynamoDB to be ready and attempt to create the table with retries
        for attempt in 1..=max_retries {
            let result = client
                .create_table()
                .table_name(&self.table)
                .attribute_definitions(
                    AttributeDefinition::builder()
                        .attribute_name("id")
                        .attribute_type(ScalarAttributeType::S)
                        .build()?,
                )
                .key_schema(
                    KeySchemaElement::builder()
                        .attribute_name("id")
                        .key_type(KeyType::Hash)
                        .build()?,
                )
                .provisioned_throughput(
                    aws_sdk_dynamodb::types::ProvisionedThroughput::builder()
                        .read_capacity_units(5)
                        .write_capacity_units(5)
                        .build()?,
                )
                .send()
                .await;

            match result {
                Ok(_) => {
                    log::info!("Table '{}' created successfully.", self.table);
                    return Ok(());
                }
                Err(err) => {
                    // Check if the table already exists
                    if let Some(service_err) = err.as_service_error() {
                        if service_err.to_string().contains("ResourceInUseException") {
                            log::warn!("Table '{}' already exists.", self.table);
                            return Ok(());
                        }
                    }

                    // If this is the final attempt, propagate the error
                    if attempt == max_retries {
                        return Err(Error::new(
                            "Failed to create DynamoDB table",
                            Some("Make sure the docker container is running and DynamoDB is available at http://localhost:8000"),
                        ).into());
                    }

                    // Otherwise, log the error and retry after a delay
                    log::warn!(
                        "Failed to create table '{}' (attempt {}/{}): {:?}, retrying...",
                        self.table,
                        attempt,
                        max_retries,
                        err
                    );

                    // Exponential backoff: initial_delay_ms * 2^(attempt-1)
                    let delay_duration =
                        std::time::Duration::from_millis(initial_delay_ms * (1 << (attempt - 1)));

                    tokio::time::sleep(delay_duration).await;
                }
            }
        }

        // This should never be reached due to the check in the loop
        Err(Error::new(
            "Failed to create DynamoDB table",
            Some("Exceeded maximum retry attempts"),
        )
        .into())
    }
}
