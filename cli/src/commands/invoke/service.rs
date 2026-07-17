mod dynamodb;
mod queue;
mod sqldb;
pub use dynamodb::LocalDynamoDB;
pub use queue::LocalQueue;
pub use sqldb::LocalSqlDB;

pub enum Service<'a> {
    DynamoDB(LocalDynamoDB),
    /// Boxed to reduce the enum size; see
    /// https://rust-lang.github.io/rust-clippy/rust-1.92.0/index.html#large_enum_variant.
    SqlDB(Box<LocalSqlDB<'a>>),
    Queue(LocalQueue),
}
