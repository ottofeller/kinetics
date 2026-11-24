pub struct Value {
    number: i32,
}
impl Value {
    fn get(&self) -> i32 {
        self.number
    }
}
#[endpoint(url_path = "/")]
fn request() -> i32 {
    Value {
        number: crate::base() + 2,
    }
    .get()
}
