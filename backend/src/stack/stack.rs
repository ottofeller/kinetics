pub struct Stack {
    pub name: String,
}

impl Stack {
    pub fn new(user_name: &str, crate_name: &str) -> Self {
        Stack {
            name: format!("{}-{}", user_name, crate_name),
        }
    }
}
