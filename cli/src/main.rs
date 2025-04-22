#![feature(once_cell_try)]
mod auth;
mod build;
mod cli;
mod client;
mod config;
mod crat;
mod deploy;
mod destroy;
mod error;
mod function;
mod invoke;
mod logger;
mod login;
mod secret;
mod stack;

use crate::cli::run;
use crate::error::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(None).await
}
