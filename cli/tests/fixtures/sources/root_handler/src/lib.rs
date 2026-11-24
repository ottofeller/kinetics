fn helper() -> i32 {
    42
}
#[worker]
fn worker() -> i32 {
    helper()
}
