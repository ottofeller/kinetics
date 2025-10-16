use crate::tools::{config::Config as KineticsConfig, resource_name};
use aws_lambda_events::sqs::{BatchItemFailure, SqsBatchResponse, SqsEvent};
use aws_sdk_sqs::operation::send_message::builders::SendMessageFluentBuilder;
use kinetics_parser::ParsedFunction;
use lambda_runtime::LambdaEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::OnceCell;

pub struct Client {
    queue: SendMessageFluentBuilder,
}

// Global cache for AWS SQS client to avoid re-initialization in Lambda
static SQS_CLIENT: OnceCell<SendMessageFluentBuilder> = OnceCell::const_new();

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
        self.queue.clone().message_body(message).send().await?;
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
        Ok(Client {
            // Initialize the SQS client just once
            queue: SQS_CLIENT
                .get_or_init(|| async {
                    let (crate_name, function_path) = std::any::type_name_of_val(&worker)
                        .split_once("::")
                        .unwrap();

                    let region = std::env::var("AWS_REGION").unwrap_or("us-east-1".to_string());

                    let queue_endpoint_url = std::env::var("KINETICS_QUEUE_ENDPOINT_URL")
                        .unwrap_or(format!("https://sqs.{region}.amazonaws.com"));

                    let queue_name = std::env::var("KINETICS_QUEUE_NAME")
                        .or_else(|_| {
                            Ok::<String, std::env::VarError>(resource_name(
                                &std::env::var("KINETICS_USERNAME")
                                    .expect("KINETICS_USERNAME is not set"),
                                crate_name,
                                &ParsedFunction::path_to_name(&function_path.replace("::", "/")),
                            ))
                        })
                        .expect("Queue name is not set");

                    let account_id = std::env::var("KINETICS_CLOUD_ACCOUNT_ID")
                        // Local SQS uses a fixed account id
                        .unwrap_or("000000000000".to_string());

                    let queue_url = format!("{queue_endpoint_url}/{account_id}/{queue_name}");

                    let config = if std::env::var("KINETICS_LOCAL_MODE").is_ok() {
                        // Redefine endpoint in local mode
                        aws_config::defaults(aws_config::BehaviorVersion::latest())
                            .endpoint_url(&queue_endpoint_url)
                            .load()
                            .await
                    } else {
                        aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await
                    };

                    println!("Initializing queue client for {queue_name}");

                    aws_sdk_sqs::Client::new(&config)
                        .send_message()
                        .queue_url(queue_url)
                })
                .await
                .to_owned(),
        })
    }
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
            sqs_batch_response
                .batch_item_failures
                .push(BatchItemFailure {
                    item_identifier: id.clone(),
                });
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
