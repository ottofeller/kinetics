mod dynamodb;
mod queue;
mod sqldb;
pub use dynamodb::LocalDynamoDB;
pub use queue::LocalQueue;
pub use sqldb::LocalSqlDB;

pub enum Service<'a> {
    DynamoDB(LocalDynamoDB),
    SqlDB(LocalSqlDB<'a>),
    Queue(LocalQueue),
}
