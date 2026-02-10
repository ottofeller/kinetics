mod runner;
use crate::runner::{Runnable, Runner};
use runner::LoginRunner;

#[derive(clap::Args, Clone)]
pub(crate) struct LoginCommand {
    /// Your registered email address
    #[arg()]
    email: String,
}

impl Runnable for LoginCommand {
    fn runner(&self) -> impl Runner {
        LoginRunner {
            command: self.clone(),
        }
    }
}
