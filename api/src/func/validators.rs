use crate::project::Project;

pub(super) fn validate_names(project: &Project, functions: &[&str]) -> Option<Vec<String>> {
    let mut errors = Vec::new();

    if project.name.trim().is_empty() {
        errors.push("Invalid \"project\". Must not be empty.".into());
    }

    if functions.iter().any(|name| name.trim().is_empty()) {
        errors.push("Invalid \"function_name\". Must not be empty.".into());
    }

    if errors.is_empty() {
        None
    } else {
        Some(errors)
    }
}
