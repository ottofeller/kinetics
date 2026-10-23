use once_cell::sync::Lazy;
use regex::Regex;

static EMAIL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$",
    )
    .expect("Failed to init regexp")
});

pub struct Email;

impl Email {
    pub fn validate(email: &str) -> bool {
        EMAIL_REGEX.is_match(email)
    }

    pub fn message() -> String {
        "Invalid \"email\". Must be a valid email address.".into()
    }
}
