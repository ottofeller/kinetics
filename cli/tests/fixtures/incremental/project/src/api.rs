use markers::endpoint;

#[endpoint(url_path = "/")]
fn request() -> i32 {
    crate::shared::base() + 20
}
