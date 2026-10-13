use crate::tools::{config::Config as KineticsConfig, runtime};
use aws_lambda_events::sqs::{BatchItemFailure, SqsBatchResponse, SqsEvent};
use aws_sdk_sqs::operation::send_message::builders::SendMessageFluentBuilder;
use eyre::{Context, Ok, OptionExt};
use lambda_runtime::LambdaEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{OnceCell, RwLock};

#[derive(Clone)]
pub struct Client {
    queue: SendMessageFluentBuilder,
}

static SQS_CLIENT_CACHE: OnceCell<Arc<RwLock<HashMap<String, Client>>>> = OnceCell::const_new();

/// A queue client
///
/// Used to send items to the worker queue.
impl Client {
    pub fn new(queue: SendMessageFluentBuilder) -> Self {
        Client { queue }
    }

    /// Send a message to the queue
    ///
    /// Return Ok(()) if operation succeeds
    pub async fn send(
        &self,
        message: impl ::std::convert::Into<::std::string::String>,
    ) -> eyre::Result<()> {
        self.send_request(self.queue.clone().message_body(message), None)
            .await
    }

    /// Send a message to the queue and delayed processing
    ///
    /// Message becomes available for processing after the delay period is finished.
    /// Valid values are 0 to 900 seconds (15 minutes).
    pub async fn send_with_delay(
        &self,
        message: impl ::std::convert::Into<::std::string::String>,
        delay_seconds: u32,
    ) -> eyre::Result<()> {
        self.send_with_options(
            message,
            SendOptions::Standard {
                delay_seconds: Some(delay_seconds),
            },
        )
        .await
    }

    pub async fn send_with_options(
        &self,
        message: impl Into<String>,
        options: SendOptions,
    ) -> eyre::Result<()> {
        self.send_request(self.queue.clone().message_body(message), Some(options))
            .await
    }

    async fn send_request(
        &self,
        mut queue: SendMessageFluentBuilder,
        options: Option<SendOptions>,
    ) -> eyre::Result<()> {
        let options = options.unwrap_or(SendOptions::default_for_queue(&queue));

        // Validate the options against the queue type (FIFO or Standard)
        options.validate(&queue)?;

        match options {
            SendOptions::Standard { delay_seconds } => {
                queue = queue.set_delay_seconds(delay_seconds.as_ref().map(|&d| d as i32));
            }
            SendOptions::Fifo {
                message_group_id,
                message_deduplication_id,
            } => {
                queue = queue
                    .message_group_id(message_group_id)
                    .message_deduplication_id(
                        message_deduplication_id
                            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    );
            }
        }

        queue.send().await?;
        Ok(())
    }

    /// Init the client from the reference to worker function
    ///
    /// The client is initialised just once and than reused.
    pub async fn from_worker<'a, Fut>(
        worker: impl Fn(Vec<Record>, &'a HashMap<String, String>, &'a KineticsConfig) -> Fut,
    ) -> eyre::Result<Self>
    where
        Fut:
            std::future::Future<Output = Result<Retries, Box<dyn std::error::Error + Send + Sync>>>,
    {
        let type_path = std::any::type_name_of_val(&worker);

        let (crate_name, module_path) = type_path
            .split_once("::")
            .ok_or_eyre("Failed to get the project name from a worker")?;

        Self::from_name(crate_name, module_path).await
    }

    /// Init the client from the crate name and module path of the worker fn
    pub async fn from_name(crate_name: &str, module_path: &str) -> eyre::Result<Self> {
        let cache_key = format!("{crate_name}::{module_path}");

        let cache = SQS_CLIENT_CACHE
            .get_or_init(|| async { Arc::new(RwLock::new(HashMap::new())) })
            .await;

        // Check if the client is already initialized
        let mut write_guard = cache.write().await;

        if let Some(client) = write_guard.get(&cache_key) {
            return Ok(client.clone());
        }

        let queue = {
            let is_local = std::env::var("KINETICS_IS_LOCAL").is_ok();
            let region = std::env::var("AWS_REGION").unwrap_or("us-east-1".to_string());
            let default_queue_endpoint = format!("https://sqs.{region}.amazonaws.com");
            let queue_endpoint_url = if is_local {
                std::env::var("KINETICS_QUEUE_ENDPOINT_URL").unwrap_or(default_queue_endpoint)
            } else {
                default_queue_endpoint
            };

            let config = if is_local {
                // Redefine endpoint in local mode
                aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .region(aws_config::Region::new(region.clone()))
                    .endpoint_url(&queue_endpoint_url)
                    .load()
                    .await
            } else {
                aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await
            };

            // Resolve the queue the message should be delivered to:
            // - locally - a per-worker named queue if the CLI provisioned one;
            // - locally - unnamed local queue;
            // - remotely - read the physical name from the runtime configuration.
            let worker_local_name = kinetics_parser::ParsedFunction::to_local_name(&[
                crate_name,
                &module_path.replace("::", "/"),
            ]);

            let (queue_name, account_id) = if is_local {
                // Local invocation: the CLI provisions the unnamed queue and,
                // optionally, a list of named per-worker queues.
                let generic_queue_name = std::env::var("KINETICS_QUEUE_NAME").wrap_err(
                    "Queue is not configured for local invocation. \
                             Re-run with `--with-queue` or `--with-worker` to provision one.",
                )?;

                let named_queues_raw =
                    std::env::var("KINETICS_LOCAL_QUEUE_NAMES").unwrap_or_default();

                let named_queues: Vec<&str> = named_queues_raw
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .collect();

                let queue_name = named_queues
                    .into_iter()
                    .find(|name| name.trim_end_matches(".fifo") == worker_local_name)
                    .map(str::to_owned)
                    .unwrap_or(generic_queue_name);

                let account_id = std::env::var("KINETICS_CLOUD_ACCOUNT_ID")
                    .wrap_err("KINETICS_CLOUD_ACCOUNT_ID is not set")?;

                (queue_name, account_id)
            } else {
                let runtime = runtime::load(&config).await?;
                (
                    runtime
                        .queues
                        .get(&worker_local_name)
                        .cloned()
                        .ok_or_else(|| {
                            eyre::eyre!("Queue is not configured for worker {worker_local_name}")
                        })?,
                    runtime.cloud_account_id.clone(),
                )
            };

            eprintln!("Resolved Queue cache_key={cache_key}, queue_name={queue_name}");

            aws_sdk_sqs::Client::new(&config)
                .send_message()
                // Create a full queue URL in a known format:
                // https://sqs.us-east1.amazonaws.com/000000000000/kinetics-queue-name
                .queue_url(format!("{queue_endpoint_url}/{account_id}/{queue_name}"))
        };
        let client = Self::new(queue);

        write_guard.insert(cache_key, client.clone());
        Ok(client)
    }
}

#[derive(Clone, Debug)]
pub enum SendOptions {
    Standard {
        delay_seconds: Option<u32>,
    },
    Fifo {
        message_group_id: String,
        message_deduplication_id: Option<String>,
    },
}

impl SendOptions {
    fn validate(&self, queue: &SendMessageFluentBuilder) -> eyre::Result<()> {
        let fifo = is_fifo(queue);

        match self {
            Self::Standard { delay_seconds } => {
                if fifo {
                    return Err(eyre::eyre!(
                        "Standard send options cannot be used with a FIFO queue"
                    ));
                }

                if delay_seconds.is_some_and(|delay| delay > 900) {
                    return Err(eyre::eyre!("Delay must be between 0 and 900 seconds"));
                }
            }
            Self::Fifo {
                message_group_id,
                message_deduplication_id,
            } => {
                if !fifo {
                    return Err(eyre::eyre!(
                        "FIFO send options cannot be used with a standard queue"
                    ));
                }

                if queue.get_delay_seconds().is_some() {
                    return Err(eyre::eyre!(
                        "Per-message delay is not supported for FIFO queues"
                    ));
                }

                let valid_id = |id: &str| {
                    (1..=128).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_graphic())
                };

                if !valid_id(message_group_id) {
                    return Err(eyre::eyre!(
                        "Message group ID must contain 1 to 128 visible ASCII characters"
                    ));
                }

                if message_deduplication_id
                    .as_deref()
                    .is_some_and(|id| !valid_id(id))
                {
                    return Err(eyre::eyre!(
                        "Message deduplication ID must contain 1 to 128 visible ASCII characters"
                    ));
                }
            }
        }

        Ok(())
    }

    fn default_for_queue(queue: &SendMessageFluentBuilder) -> Self {
        if is_fifo(queue) {
            SendOptions::Fifo {
                message_group_id: queue
                    .get_message_group_id()
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
                message_deduplication_id: queue.get_message_deduplication_id().clone(),
            }
        } else {
            SendOptions::Standard {
                delay_seconds: queue.get_delay_seconds().map(|delay| delay as u32),
            }
        }
    }
}

fn is_fifo(queue: &SendMessageFluentBuilder) -> bool {
    queue
        .get_queue_url()
        .as_deref()
        .and_then(|queue_url| url::Url::parse(queue_url).ok())
        .is_some_and(|url| url.path().trim_end_matches('/').ends_with(".fifo"))
}

/// Items to be retried by worker queue
///
/// Worker function must return a Retries struct with ids of items that need
/// to be retried.
#[derive(Default)]
pub struct Retries {
    ids: Vec<String>,
}

impl Retries {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn add(&mut self, item: &str) {
        self.ids.push(item.to_string());
    }

    /// Serialize to the format which can be understood by queue API
    pub fn collect(&self) -> SqsBatchResponse {
        let mut sqs_batch_response = SqsBatchResponse::default();

        for id in self.ids.iter() {
            // Construct through Default, because BatchItemFailure is non_exhaustive
            let mut item = BatchItemFailure::default();
            item.item_identifier = id.to_owned();
            sqs_batch_response.batch_item_failures.push(item);
        }

        sqs_batch_response
    }
}

/// A record received from a queue
#[derive(Deserialize, Serialize, Debug)]
pub struct Record {
    #[serde(default)]
    pub message_id: Option<String>,

    #[serde(default)]
    pub body: Option<String>,
}

impl Record {
    pub fn from_sqsevent(event: LambdaEvent<SqsEvent>) -> eyre::Result<Vec<Record>> {
        Ok(event
            .payload
            .records
            .iter()
            .map(|r| Record {
                message_id: r.message_id.clone(),
                body: r.body.clone(),
            })
            .collect())
    }
}
