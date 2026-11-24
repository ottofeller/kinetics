use crate::error::Error;
use aws_config::BehaviorVersion;
use aws_sdk_s3::error::ProvideErrorMetadata;
use std::path::PathBuf;
use std::time::Duration;

const DOCKER_COMPOSE_SNIPPET: &str = r#"
local-s3:
    image: "andrewgaul/s3proxy:sha-f892634"
    ports:
        - "8080:80"
    volumes:
        - "{{STORAGE_VOLUME_PATH}}:/data/{{STORAGE_BUCKET}}"
"#;

/// Manage local S3 (container and bucket)
///
/// The mount targets the container's bucket directory, so objects appear as
/// plain files directly under the mounted folder.
pub struct LocalS3 {
    volume_path: PathBuf,
}

impl LocalS3 {
    /// Bucket served by the local S3 container; must match `KINETICS_STORAGE_BUCKET`.
    pub const BUCKET: &str = "kinetics-local";

    /// Endpoint of the local S3 container; must match the port mapping above.
    pub const ENDPOINT: &str = "http://localhost:8080";

    /// Access key id the local S3 container authenticates.
    pub const IDENTITY: &str = "local-identity";

    /// Secret access key of the local S3 container.
    pub const CREDENTIAL: &str = "local-credential";

    /// Region the local S3 signs with.
    /// The value is arbitrary for s3proxy,
    /// so kept at the conventional default.
    pub const REGION: &str = "us-east-1";

    pub fn new(volume_path: PathBuf) -> Self {
        Self { volume_path }
    }

    pub fn docker_compose_snippet(&self) -> String {
        DOCKER_COMPOSE_SNIPPET
            .replace(
                "{{STORAGE_VOLUME_PATH}}",
                &self.volume_path.display().to_string(),
            )
            .replace("{{STORAGE_BUCKET}}", Self::BUCKET)
    }

    /// Provision the bucket with retry mechanism for handling container startup
    pub async fn provision(&self) -> eyre::Result<()> {
        // Explicit settings from the constants above — no config chain.
        let config = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Some(aws_sdk_s3::config::Region::new(Self::REGION)))
            .endpoint_url(Self::ENDPOINT)
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                Self::IDENTITY,
                Self::CREDENTIAL,
                None,
                None,
                "provider",
            ))
            // s3proxy answers path-style requests; virtual-hosted would need bucket DNS.
            .force_path_style(true)
            .build();

        let client = aws_sdk_s3::Client::from_conf(config);

        // Retry parameters
        let max_retries = 10;
        let retry_delay_ms = 1000;

        for attempt in 1..=max_retries {
            let result = client.create_bucket().bucket(Self::BUCKET).send().await;

            match result {
                Ok(_) => {
                    log::info!("Bucket '{}' created successfully.", Self::BUCKET);
                    return Ok(());
                }
                Err(err) => {
                    // Ok on existing bucket since the it survives container restarts.
                    let code = err
                        .as_service_error()
                        .and_then(|service_error| service_error.code());

                    if code == Some("BucketAlreadyOwnedByYou")
                        || code == Some("BucketAlreadyExists")
                    {
                        log::warn!("Bucket '{}' already exists.", Self::BUCKET);
                        return Ok(());
                    }

                    if attempt == max_retries {
                        return Err(Error::new(
                            "Failed to create local storage bucket",
                            Some("Make sure the docker container is running and s3proxy is available at http://localhost:8080"),
                        ).into());
                    }

                    log::warn!(
                        "Failed to create bucket '{}' (attempt {}/{}): {code:?} {err:?}; retrying...",
                        Self::BUCKET,
                        attempt,
                        max_retries,
                    );

                    tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;
                }
            }
        }

        Err(Error::new(
            "Failed to create local storage bucket",
            Some("Exceeded maximum retry attempts"),
        )
        .into())
    }
}
