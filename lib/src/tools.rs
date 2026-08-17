pub mod config;
pub mod http;
pub mod queue;

/// Namespace included in project resource hash inputs.
const RESOURCE_NAMESPACE: &str = "kinetics";
const PROJECT_HASH_LENGTH: usize = 16;
const RESOURCE_HASH_LENGTH: usize = 12;
const PROJECT_RESOURCE_NAME_MAX_LENGTH: usize = 64;

/// Type of physical resource scoped to a project.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum ProjectResourceKind {
    /// Primary SQS queue.
    Queue,
    /// SQS dead-letter queue.
    DeadLetterQueue,
    /// SSM parameter containing a project secret.
    Secret,
    /// Lambda function.
    Lambda,
}

impl ProjectResourceKind {
    /// Short identifier used in physical resource names.
    fn token(self) -> &'static str {
        match self {
            Self::Queue => "q",
            Self::DeadLetterQueue => "d",
            Self::Secret => "s",
            Self::Lambda => "l",
        }
    }

    /// Canonical kind name included in the hash input.
    fn canonical_name(self) -> &'static str {
        match self {
            Self::Queue => "main-queue",
            Self::DeadLetterQueue => "dead-letter-queue",
            Self::Secret => "secret",
            Self::Lambda => "lambda",
        }
    }
}

/// Returns a deterministic hash segment of the requested length for a resource scope or identity.
fn resource_hash(
    owner_id: &str,
    project_name: &str,
    kind: ProjectResourceKind,
    resource_name: Option<&str>,
    hash_length: usize,
) -> String {
    let mut name = format!(
        "{RESOURCE_NAMESPACE}-{owner_id}-{project_name}-{}",
        kind.canonical_name()
    );

    if let Some(resource_name) = resource_name {
        name.push('-');
        name.push_str(resource_name);
    }

    sha256::digest(name).chars().take(hash_length).collect()
}

/// Returns the common prefix for a resource kind, owner, and project.
pub fn project_resource_prefix(
    kind: ProjectResourceKind,
    owner_id: &str,
    project_name: &str,
) -> String {
    format!(
        "{}-{}-",
        kind.token(),
        resource_hash(owner_id, project_name, kind, None, PROJECT_HASH_LENGTH)
    )
}

/// Returns a physical resource name composed of project and resource hashes and a readable suffix.
///
/// The suffix contains only ASCII alphanumeric characters, and the complete name is at most
/// 64 bytes.
pub fn project_resource_name(
    kind: ProjectResourceKind,
    owner_id: &str,
    project_name: &str,
    resource_name: &str,
) -> String {
    let prefix = project_resource_prefix(kind, owner_id, project_name);

    let resource_hash = resource_hash(
        owner_id,
        project_name,
        kind,
        Some(resource_name),
        RESOURCE_HASH_LENGTH,
    );

    let readable_name_max_length =
        PROJECT_RESOURCE_NAME_MAX_LENGTH.saturating_sub(prefix.len() + resource_hash.len() + 1);

    let readable_name = resource_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(readable_name_max_length)
        .collect::<String>();

    let mut resource_name = format!("{prefix}{resource_hash}");

    if !readable_name.is_empty() {
        resource_name.push('-');
        resource_name.push_str(&readable_name);
    }

    resource_name
}

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
