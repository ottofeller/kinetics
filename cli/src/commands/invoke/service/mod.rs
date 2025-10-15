mod dynamodb;
mod sqldb;
pub use dynamodb::LocalDynamoDB;
pub use sqldb::LocalSqlDB;

pub enum Service {
    DynamoDB(LocalDynamoDB),
    SqlDB(LocalSqlDB),
}
