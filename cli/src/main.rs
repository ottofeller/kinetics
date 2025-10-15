mod build;
mod cli;
mod client;
mod config;
mod crat;
mod credentials;
mod deploy;
mod error;
mod function;
mod init;
mod invoke;
mod logger;
mod login;
mod logout;
mod process;
mod project;
mod secret;
mod commands;
pub mod sqldb;
use crate::cli::run;
use crate::error::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(None).await
}
