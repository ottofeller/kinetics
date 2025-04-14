pub mod stack;
pub mod template;

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
