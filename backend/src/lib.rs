pub mod auth;
pub mod crat;
pub mod deploy;
pub mod env;
pub mod function;
pub mod json;
pub mod secret;
pub mod template;
pub mod upload;
pub mod usage;

#[derive(Clone, Debug)]
pub struct Queue {
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
