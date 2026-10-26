use crate::tools::runtime;
use aws_config::SdkConfig;
use aws_sdk_s3::primitives::{ByteStream, DateTime};
use eyre::Context;
use tokio::sync::OnceCell;

/// An object in the project storage
#[derive(Clone, Debug)]
pub struct ObjectSummary {
    pub key: String,
    pub size: i64,
    pub last_modified: Option<DateTime>,
}

static STORAGE_CLIENT: OnceCell<Client> = OnceCell::const_new();

/// File storage client
#[derive(Clone)]
pub struct Client {
    client: aws_sdk_s3::Client,
    bucket: String,
    prefix: String,
}

impl Client {
    /// Create a client with the default SDK configuration
    pub async fn new() -> eyre::Result<Self> {
        let client = STORAGE_CLIENT
            .get_or_try_init(|| async {
                let sdk_config =
                    aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                Self::from_config(&sdk_config).await
            })
            .await?;

        Ok(client.clone())
    }

    /// Create a client from the SDK configuration
    pub async fn from_config(sdk_config: &SdkConfig) -> eyre::Result<Self> {
        let runtime = runtime::load(sdk_config).await?;

        Ok(Self {
            client: aws_sdk_s3::Client::new(sdk_config),
            bucket: runtime.storage_bucket.clone(),
            prefix: runtime.storage_prefix.clone(),
        })
    }

    /// Upload an object, overwriting any existing object under the key
    pub async fn upload(&self, key: &str, body: impl Into<ByteStream>) -> eyre::Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(self.full_key(key)?)
            .body(body.into())
            .send()
            .await
            .wrap_err_with(|| format!("Failed to upload object {key}"))?;

        Ok(())
    }

    /// Download an object
    pub async fn download(&self, key: &str) -> eyre::Result<Vec<u8>> {
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.full_key(key)?)
            .send()
            .await
            .wrap_err_with(|| format!("Failed to download object {key}"))?;

        let body = response
            .body
            .collect()
            .await
            .wrap_err_with(|| format!("Failed to read object {key}"))?;

        Ok(body.into_bytes().to_vec())
    }

    /// Delete an object; succeeds when the key does not exist
    pub async fn delete(&self, key: &str) -> eyre::Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(self.full_key(key)?)
            .send()
            .await
            .wrap_err_with(|| format!("Failed to delete object {key}"))?;

        Ok(())
    }

    /// List all project objects
    pub async fn list(&self) -> eyre::Result<Vec<ObjectSummary>> {
        let mut summaries = Vec::new();
        let mut continuation_token = None;

        loop {
            let response = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&self.prefix)
                .set_continuation_token(continuation_token)
                .send()
                .await
                .wrap_err("Failed to list storage objects")?;

            summaries.extend(response.contents().iter().filter_map(|object| {
                Some(ObjectSummary {
                    key: object.key()?.strip_prefix(&self.prefix)?.to_string(),
                    size: object.size().unwrap_or_default(),
                    last_modified: object.last_modified().cloned(),
                })
            }));

            match response.next_continuation_token() {
                Some(token) => continuation_token = Some(token.to_string()),
                None => break,
            }
        }

        Ok(summaries)
    }

    /// Check whether an object exists in the project storage
    pub async fn exists(&self, key: &str) -> eyre::Result<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(self.full_key(key)?)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(error)
                if error
                    .raw_response()
                    .is_some_and(|response| response.status().as_u16() == 404) =>
            {
                Ok(false)
            }
            Err(error) => Err(error).wrap_err_with(|| format!("Failed to check object {key}")),
        }
    }

    /// Validate the caller's key and compose the full storage key
    fn full_key(&self, key: &str) -> eyre::Result<String> {
        if key.is_empty() {
            eyre::bail!("Storage key must not be empty");
        }

        if key.chars().any(char::is_control) {
            eyre::bail!("Storage key {key:?} must not contain control characters");
        }

        Ok(format!("{}{key}", self.prefix))
    }
}
