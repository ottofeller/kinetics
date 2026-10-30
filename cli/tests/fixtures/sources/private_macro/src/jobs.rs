use crate::answer;
#[worker]
fn worker() -> i32 {
    answer!()
}
