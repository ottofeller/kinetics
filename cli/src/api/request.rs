/// Validate fields of a http request struct
pub(crate) trait Validate {
    /// Returns vector of errors or None if all valid
    fn validate(&self) -> Option<Vec<String>>;
}
