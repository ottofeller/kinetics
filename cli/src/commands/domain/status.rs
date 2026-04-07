use crate::error::Error;
use crate::runner::{Runnable, Runner};
use crate::writer::Writer;

#[derive(clap::Args, Clone)]
pub(crate) struct StatusCommand;

impl Runnable for StatusCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        StatusRunner {
            command: self.clone(),
            writer,
        }
    }
}

struct StatusRunner<'a> {
    command: StatusCommand,
    writer: &'a Writer,
}

impl Runner for StatusRunner<'_> {
    async fn run(&mut self) -> Result<(), Error> {
        self.writer.text("Requesting status...")?;
        Ok(())
    }
}
