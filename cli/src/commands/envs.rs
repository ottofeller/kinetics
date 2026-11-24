pub mod list;

use crate::commands::envs::list::ListCommand;
use crate::error::Error;
use clap::Subcommand;
use kinetics_api::envs::{FunctionResult, Response};

pub(crate) fn check_results(response: &Response, message: &str) -> Result<(), Error> {
    let errors: Vec<_> = response
        .results
        .iter()
        .filter_map(|(name, result)| match result {
            FunctionResult::Error { error } => Some(match &error.details {
                Some(details) => format!("{name}: {}\n{details}", error.message),
                None => format!("{name}: {}", error.message),
            }),
            FunctionResult::Success { .. } => None,
        })
        .collect();

    if errors.is_empty() {
        return Ok(());
    }

    let details = format!(
        "{} succeeded, {} failed.\n\n{}",
        response.results.len() - errors.len(),
        errors.len(),
        errors.join("\n\n")
    );
    Err(Error::new(message, Some(&details))
        .with_code("envs_operation_failed")
        .with_data(serde_json::to_value(response).map_err(eyre::Report::from)?))
}

#[derive(Subcommand)]
pub(crate) enum EnvsCommands {
    /// List all environment variables for all functions
    List(ListCommand),
}
