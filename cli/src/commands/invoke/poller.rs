use super::service::LocalQueue;
use eyre::WrapErr;
use tokio::sync::mpsc;

/// SQS long-poll wait time in seconds.
///
/// Short on purpose: the drain phase only needs to catch messages the main
/// function enqueued before polling started, so a long wait would just delay
/// idle detection on empty queues (one full window per worker).
const WAIT_TIME_SECONDS: i32 = 2;

/// Polls a local queue and forwards each received message over a channel.
pub struct LocalQueuePoller {
    /// Receiver end of the channel the polling task pushes messages into.
    receiver: mpsc::Receiver<String>,

    /// Handle of the background polling task.
    handle: tokio::task::JoinHandle<eyre::Result<()>>,
}

impl LocalQueuePoller {
    pub fn new(queue: LocalQueue) -> Self {
        let (sender, receiver) = mpsc::channel(64);
        let handle = tokio::spawn(async move { Self::poll_loop(queue, sender).await });

        Self { receiver, handle }
    }

    /// Drain the queue messages received by the polling task.
    ///
    /// Collects messages from the channel until it stays idle for a while.
    /// The polling task itself keeps running (and holding the channel sender),
    /// so we cannot wait for the channel to close — an idle timeout
    /// is the only reliable stop condition.
    pub async fn drain_queue(&mut self) -> eyre::Result<Vec<String>> {
        // Must exceed the SQS long-poll `wait_time_seconds` so that a poll
        // returning an empty batch doesn't look like an idle stop.
        let idle_timeout = std::time::Duration::from_secs(WAIT_TIME_SECONDS as u64 + 1);

        let mut messages = Vec::new();
        loop {
            match tokio::time::timeout(idle_timeout, self.receiver.recv()).await {
                // Got a message; keep draining.
                Ok(Some(body)) => messages.push(body),
                // Polling task has stopped.
                Ok(None) => break,
                // No new messages within the timeout window: assume drained.
                Err(_) => break,
            }
        }

        // The polling task may have died (SQS unreachable, bad URL, etc.),
        // which closes the channel and looks like a drained queue. Surface
        // its error instead of reporting an empty queue.
        if self.handle.is_finished() {
            match (&mut self.handle).await {
                Ok(ok @ Ok(())) => ok,
                Ok(err) => err.wrap_err("Local queue polling failed"),
                Err(err) => Err(eyre::eyre!("Local queue polling task failed: {err}")),
            }?;
        }

        Ok(messages)
    }

    /// Stop the background polling task.
    pub fn abort(&self) {
        self.handle.abort();
    }

    /// Long-poll the queue and forward each received message body.
    async fn poll_loop(queue: LocalQueue, sender: mpsc::Sender<String>) -> eyre::Result<()> {
        let client = queue.client().await;
        let queue_url = format!(
            "{}/{}/{}",
            queue.endpoint_url(),
            queue.account_id(),
            queue.name()
        );

        println!(
            "\n{} queue {}...\n",
            console::style("Polling").bold(),
            queue.name()
        );

        loop {
            let receive = client
                .receive_message()
                .queue_url(&queue_url)
                .max_number_of_messages(10)
                .wait_time_seconds(WAIT_TIME_SECONDS)
                .send()
                .await
                .wrap_err("Failed to receive messages from the local queue")?;

            let messages = receive.messages.unwrap_or_default();
            for message in messages {
                // Delete the message from the queue right away. Without this,
                // the message becomes visible again after the visibility
                // timeout and the drain loop would never go idle.
                if let Some(receipt_handle) = message.receipt_handle.clone() {
                    client
                        .delete_message()
                        .queue_url(&queue_url)
                        .receipt_handle(receipt_handle)
                        .send()
                        .await
                        .wrap_err("Failed to delete message from the local queue")?;
                }

                let body = message.body.unwrap_or_default();
                // If the consumer is gone, stop polling.
                if sender.send(body).await.is_err() {
                    return Ok(());
                }
            }
        }
    }
}
