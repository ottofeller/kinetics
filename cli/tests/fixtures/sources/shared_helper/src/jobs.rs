pub fn call() -> i32 {
    worker()
}
#[worker]
fn worker() -> i32 {
    42
}
