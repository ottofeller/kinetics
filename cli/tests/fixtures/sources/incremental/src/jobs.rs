fn helper() -> i32 {
    1
}
#[worker]
fn worker() -> i32 {
    helper()
}
