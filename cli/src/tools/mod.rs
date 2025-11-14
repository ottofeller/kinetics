pub mod config;
pub mod http;
pub mod queue;

/// Unique resource name
///
/// Construct a readable name by escaping non-ascii chars, and appending a hash of
/// a full unescaped name (for uniqueness reason).
///
/// The string is truncated to 64 symbols, which is the maximum length
/// for a resource name in most platforms.
pub fn resource_name(user_name: &str, project_name: &str, resource_name: &str) -> String {
    format!(
        "{}{}",
        // Keep readable name to distinguish resources in the dahsboards
        resource_name
            .chars()
            .take(32)
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase(),
        // Add hash for uniqueness
        sha256::digest(format!("{}-{}-{}", user_name, project_name, resource_name))
            .to_string()
            .chars()
            .take(32)
            .collect::<String>(),
    )
}
