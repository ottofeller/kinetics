pub mod deploy;
pub mod crat;
pub mod function;
pub mod template;
pub mod secret;
pub mod upload;
pub mod json;

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
