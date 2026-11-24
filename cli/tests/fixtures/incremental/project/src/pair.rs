use markers::endpoint;

#[endpoint(url_path = "/first")]
fn first() -> i32 {
    crate::shared::base() + 30
}

#[endpoint(url_path = "/second")]
fn second() -> i32 {
    crate::shared::base() + 40
}
