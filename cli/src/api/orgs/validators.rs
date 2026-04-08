use once_cell::sync::Lazy;
use regex::Regex;

static NAME_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z\-_]{2,32}$").expect("Failed to init regexp"));

pub struct Name;

impl Name {
    pub fn validate(name: &str) -> bool {
        NAME_REGEX.is_match(name)
    }

    pub fn message() -> String {
        "Invalid \"name\". Must be 2-32 characters long and contain only letters (a-z, A-Z), hyphens (-), and underscores (_).".into()
    }
}
