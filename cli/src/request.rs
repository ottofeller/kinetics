pub(crate) trait ValidateRequest {
    /// Validate Request fields
    ///
    /// Returns vector of errors or None if all valid
    fn validate(&self) -> Option<Vec<String>>;
}
