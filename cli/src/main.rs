mod cli;
mod client;
mod commands;
pub mod config;
mod crat;
mod credentials;
mod error;
mod function;
mod logger;
mod process;
mod project;
mod secret;
pub mod sqldb;
pub mod tools;
use crate::cli::run;
use crate::error::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(None).await
}
