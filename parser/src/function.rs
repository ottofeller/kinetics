use crate::params::Params;
use color_eyre::eyre;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// The kind of function, without parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Role {
    Endpoint,
    Cron,
    Worker,
}

impl Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Role::Endpoint => "endpoint",
            Role::Cron => "cron",
            Role::Worker => "worker",
        };

        write!(f, "{}", str)
    }
}

/// Represents a function in the source code
#[derive(Debug, Clone)]
pub struct ParsedFunction {
    /// Name of the function, parsed from the function definition
    pub rust_function_name: String,

    /// Path to the file where function is defined
    pub relative_path: String,

    /// The kind of function (endpoint, cron, or worker), without parameters
    pub role: Role,

    /// The workload-specific parameters parsed from the kinetics macro attribute
    pub params: Params,
}

impl ParsedFunction {
    /// Convert a path to CamelCase name
    pub fn path_to_name(path: &str) -> String {
        path.split(&['.', '/'])
            .filter(|s| !s.eq(&"rs"))
            .map(|s| match s.chars().next() {
                Some(first) => first.to_uppercase().collect::<String>() + &s[1..],
                None => String::new(),
            })
            .collect::<String>()
            .replacen("Src", "", 1)
    }

    /// Generate lambda function name out of Rust function name or macro attribute
    ///
    /// By default use the Rust function plus crate path as the function name. Convert
    /// some-name to SomeName, and do other transformations in order to comply with Lambda
    /// function name requirements.
    pub fn func_name(&self, is_local: bool) -> eyre::Result<String> {
        let rust_name = &self.rust_function_name;
        let full_path = format!("{}/{rust_name}", self.relative_path);
        let default_func_name = Self::path_to_name(&full_path);
        let name = self.params.name().unwrap_or(&default_func_name);

        if name.len() > 64 {
            Err(eyre::eyre!(
                "Function name is longer than 64 chars: {}",
                name
            ))
        } else {
            // TODO Check the name for uniqueness
            Ok(format!("{}{}", name, if is_local { "Local" } else { "" }))
        }
    }
}
