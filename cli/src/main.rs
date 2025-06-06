mod build;
mod cli;
mod client;
mod config;
mod crat;
mod credentials;
mod deploy;
mod destroy;
mod error;
mod function;
mod init;
mod invoke;
mod list;
mod logger;
mod login;
mod logout;
mod logs;
mod process;
mod secret;
use crate::cli::run;
use crate::error::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(None).await
}
