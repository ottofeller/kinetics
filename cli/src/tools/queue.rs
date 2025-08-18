use aws_sdk_sqs::operation::send_message::builders::SendMessageFluentBuilder;

pub struct Client {
    queue: SendMessageFluentBuilder,
}

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
}
