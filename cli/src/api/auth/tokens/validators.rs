use regex::Regex;
pub(crate) struct Name;

impl Name {
    pub fn validate(name: &str) -> bool {
        Regex::new(r"^[a-zA-Z\-]{2,32}$")
            .expect("Failed to init regexp")
            .is_match(name)
    }

    pub fn message() -> String {
        "Invalid \"name\". Must be 2-32 characters long and contain only letters (a-z, A-Z) and hyphens (-).".into()
    }
}
