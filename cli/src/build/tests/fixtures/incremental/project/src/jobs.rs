use markers::worker;

fn helper() -> i32 {
    10
}

#[worker]
fn worker() -> i32 {
    crate::shared::base() + helper()
}
