/// Replace any unwanted character in resource name
/// with its uppercase-alpha counterpart
pub fn escape_resource_name(name: &str) -> String {
    name.replace("@", "AT")
        .replace(".", "DOT")
        .replace("-", "HYPHEN")
        .replace("_", "UNDRSC")
}
