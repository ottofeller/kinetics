use crate::tools::config::Config as KineticsConfig;
use crate::tools::resource_name;
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

                    let queue_name = resource_name(
                        &std::env::var("KINETICS_USERNAME").unwrap(),
                        &crate_name,
                        &ParsedFunction::path_to_name(&function_path.replace("::", "/")),
                    );

                    println!("Initializing queue client for {queue_name}");

                    let queue_url = format!(
                        "https://sqs.us-east-1.amazonaws.com/{}/{}",
                        &std::env::var("KINETICS_CLOUD_ACCOUNT_ID").unwrap(),
                        queue_name
                    );

                    let config =
                        aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;

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
