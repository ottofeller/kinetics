use once_cell::sync::Lazy;
use regex::Regex;

/// DNS name per RFC 1035: each label is 1–63 chars of `[a-z0-9-]` and cannot start or end with a
/// hyphen, followed by a TLD of 2+ letters.
static NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?i)(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,}$")
        .expect("Failed to init regexp")
});

pub struct Name;

impl Name {
    pub fn validate(domain: &str) -> bool {
        domain.len() <= 253 && NAME_REGEX.is_match(domain)
    }

    pub fn message() -> String {
        "Invalid \"domain\". Must be a valid DNS name (e.g. example.com).".into()
    }
}
