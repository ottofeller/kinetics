use crate::tools::{
    config::Config,
    http::Body,
    queue::{Record, Retries},
};
use std::collections::HashMap;

async fn endpoint_handler(
    _event: http::Request<Body>,
    _secrets: &HashMap<String, String>,
    _config: &Config,
) -> Result<http::Response<String>, tower::BoxError> {
    Ok(http::Response::new(String::new()))
}

async fn worker_handler(
    _records: Vec<Record>,
    _secrets: &HashMap<String, String>,
    _config: &Config,
) -> Result<Retries, tower::BoxError> {
    Ok(Retries::new())
}

async fn cron_handler(
    _secrets: &HashMap<String, String>,
    _config: &Config,
) -> Result<(), tower::BoxError> {
    Ok(())
}

#[test]
fn endpoint_remote_compiles() {
    crate::endpoint_main!(remote, endpoint_handler, "/test/{id}");
    let _entrypoint = main;
}

#[test]
fn endpoint_local_compiles() {
    crate::endpoint_main!(local, endpoint_handler, "/test/{id}");
    let _entrypoint = main;
}

#[test]
fn worker_remote_compiles() {
    crate::worker_main!(remote, worker_handler);
    let _entrypoint = main;
}

#[test]
fn worker_local_compiles() {
    crate::worker_main!(local, worker_handler);
    let _entrypoint = main;
}

#[test]
fn cron_remote_compiles() {
    crate::cron_main!(remote, cron_handler);
    let _entrypoint = main;
}

#[test]
fn cron_local_compiles() {
    crate::cron_main!(local, cron_handler);
    let _entrypoint = main;
}
