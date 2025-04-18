mod auth;
mod build;
mod cli;
mod client;
mod config;
mod crat;
mod deploy;
mod destroy;
mod function;
mod invoke;
mod login;
mod secret;
mod stack;

use crate::cli::run;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    run(None).await
}
