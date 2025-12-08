mod cli;
mod client;
mod commands;
mod config;
mod credentials;
mod error;
mod function;
mod logger;
mod process;
pub mod project;
mod secrets;
pub mod sqldb;
pub mod tools;
use crate::cli::run;
use crate::error::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(None).await
}
