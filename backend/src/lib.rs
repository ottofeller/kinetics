pub mod auth;
pub mod config;
pub mod env;
pub mod json;
pub mod stack;
pub mod template;
pub mod upload;
pub mod usage;
pub mod user;

#[derive(Clone, Debug)]
pub struct Queue {
    alias: String,
    cfn_name: Option<String>,
    name: String,
    concurrency: u32,
}

#[derive(Clone, Debug)]
pub struct KvDb {
    name: String,
}

#[derive(Clone, Debug)]
pub enum Resource {
    Queue(Queue),
    KvDb(KvDb),
}
