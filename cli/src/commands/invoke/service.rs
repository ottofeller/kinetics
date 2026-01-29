mod dynamodb;
mod queue;
mod sqldb;
pub use dynamodb::LocalDynamoDB;
pub use queue::LocalQueue;
pub use sqldb::LocalSqlDB;

pub enum Service {
    DynamoDB(LocalDynamoDB),
    SqlDB(LocalSqlDB),
    Queue(LocalQueue),
}
