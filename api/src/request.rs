use serde::Serialize;
use serde_json::Value;

/// A redacted value used for logging sensitive data
const REDACTED: &str = "[REDACTED]";

/// A list of keywords used to identify sensitive data in log output
const REDACTED_KEYWORDS: &[&str] = &[
    "secret",
    "secrets",
    "apikey",
    "password",
    "passwd",
    "token",
    "credential",
    "authorization",
];

/// Validates fields of an HTTP request struct before sending it
pub trait Validate {
    /// Returns validation errors, or `None` when the request is valid
    fn validate(&self) -> Option<Vec<String>>;
}

/// Serializes a value as pretty JSON with sensitive fields redacted for logs
pub fn to_log_safe_string_pretty<T: Serialize>(value: &T) -> serde_json::Result<String> {
    let mut value = serde_json::to_value(value)?;
    redact_matching(&mut value);
    serde_json::to_string_pretty(&value)
}

/// Redacts sensitive values in a JSON value for logging purposes
///
/// Recursively traverses the JSON value, redacting any sensitive values found
fn redact_matching(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if is_sensitive_key(key) {
                    redact_value(value);
                } else {
                    redact_matching(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_matching(value);
            }
        }
        _ => {}
    }
}

/// Redacts the value of a sensitive key, replacing it with a redacted placeholder
fn redact_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for value in map.values_mut() {
                redact_value(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_value(value);
            }
        }
        _ => {
            *value = Value::String(REDACTED.into());
        }
    }
}

/// Checks if a key is a sensitive key that should be redacted
fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|char| char.is_ascii_alphanumeric())
        .flat_map(|char| char.to_lowercase())
        .collect::<String>();

    REDACTED_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword))
}
