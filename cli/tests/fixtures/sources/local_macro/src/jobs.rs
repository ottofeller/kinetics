macro_rules! answer {
    () => {
        42
    };
}
#[worker]
fn worker() -> i32 {
    answer!()
}
