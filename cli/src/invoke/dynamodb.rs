use crate::error::Error;
use crate::process::Process;
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, KeySchemaElement, KeyType, ScalarAttributeType,
};
use eyre::WrapErr;
use std::{
    path::{Path, PathBuf},
    process::{self, Stdio},
};

const DOCKER_COMPOSE_FILE: &str = r#"
version: "3.8"
services:
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
    /// Path to .kinetics dir
    build_path: PathBuf,

    /// A flag indicating the instanse was started
    is_started: bool,
}

impl LocalDynamoDB {
    pub fn new(build_path: &Path) -> Self {
        Self {
            build_path: build_path.to_owned(),
            is_started: false,
        }
    }

    /// Start DynamoDB container
    pub fn start(&mut self) -> eyre::Result<()> {
        let dest = self.docker_compose_path();

        std::fs::write(&dest, DOCKER_COMPOSE_FILE)
            .inspect_err(|e| {
                log::error!("Failed to write DOCKER_COMPOSE_FILE to {:?}: {}", dest, e)
            })
            .wrap_err(Error::new(
                "Failed to set up Docker",
                Some(&format!("Make sure you can write to {dest:?}")),
            ))?;

        // Config file functionality must ensure that the root dirs are all valid
        let file_path = dest.to_string_lossy();

        let child = process::Command::new("docker-compose")
            .args(&["-f", &file_path, "up", "-d"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .wrap_err("Failed to execute docker-compose")?;

        self.is_started = true;

        let mut process = Process::new(child);
        let status = process.log()?;

        if !status.success() {
            process.print_error();

            return Err(Error::new(
                "Failed to start DynamoDB container",
                Some("Make sure the docker is installed and running."),
            )
            .into());
        }

        Ok(())
    }

    /// Stop DynamoDB container
    pub fn stop(&self) -> eyre::Result<()> {
        if !self.is_started {
            // self.start was not called
            return Ok(());
        }

        let status = process::Command::new("docker-compose")
            .args(&["-f", &self.docker_compose_path().to_string_lossy(), "down"])
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .inspect_err(|e| log::error!("Error: {}", e))
            .wrap_err("Failed to execute docker-compose")?;

        if !status.success() {
            return Err(eyre::eyre!(
                "docker-compose command failed with exit code: {}",
                status
            ));
        }

        Ok(())
    }

    /// Provision table with retry mechanism for handling connection issues
    pub async fn provision(&self, table: &str) -> eyre::Result<()> {
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
                .table_name(table)
                .attribute_definitions(
                    AttributeDefinition::builder()
                        .attribute_name("id")
                        .attribute_type(ScalarAttributeType::S)
                        .build()
                        .unwrap(),
                )
                .key_schema(
                    KeySchemaElement::builder()
                        .attribute_name("id")
                        .key_type(KeyType::Hash)
                        .build()
                        .unwrap(),
                )
                .provisioned_throughput(
                    aws_sdk_dynamodb::types::ProvisionedThroughput::builder()
                        .read_capacity_units(5)
                        .write_capacity_units(5)
                        .build()
                        .unwrap(),
                )
                .send()
                .await;

            match result {
                Ok(_) => {
                    log::info!("Table '{}' created successfully.", table);
                    return Ok(());
                }
                Err(err) => {
                    // Check if the table already exists
                    if let Some(service_err) = err.as_service_error() {
                        if service_err.to_string().contains("ResourceInUseException") {
                            log::warn!("Table '{}' already exists.", table);
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
                        table,
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

    /// Path to docker-compose.yml file
    fn docker_compose_path(&self) -> PathBuf {
        self.build_path.join("docker-compose.yml")
    }
}

impl Drop for LocalDynamoDB {
    fn drop(&mut self) {
        self.stop().unwrap();
    }
}
