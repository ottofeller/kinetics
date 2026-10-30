struct Value(i32);
impl Value {
    fn get(&self) -> i32 {
        self.0
    }
}
fn helper() -> i32 {
    Value(42).get()
}
#[worker]
fn worker() -> i32 {
    helper()
}
#[dlq_worker]
fn retry() -> i32 {
    worker()
}
