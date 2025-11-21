pub mod docker;
mod local;
mod remote;
pub mod service;
use super::build::prepare_functions;
use crate::config::build_config;
use crate::function::Function;
use crate::project::Project;
use std::path::PathBuf;

/// Invoke the function either locally or remotely
#[allow(clippy::too_many_arguments)]
pub async fn invoke(
    function_name: &str,
    project: &Project,
    payload: &str,
    headers: &str,
    url_path: &str,

    // DynamoDbB table to provision, only relevant for local invocations
    table: Option<&str>,

    is_local: bool,
    is_sqldb_enabled: bool,
    is_queue_enabled: bool,
) -> eyre::Result<()> {
    // Get function names as well as pull all updates from the code.
    let all_functions = prepare_functions(
        PathBuf::from(build_config()?.kinetics_path),
        project,
        &[function_name.into()],
    )?;
    let function = Function::find_by_name(&all_functions, function_name)?;

    if is_local {
        local::invoke(
            &function,
            project,
            payload,
            headers,
            url_path,
            table,
            is_sqldb_enabled,
            is_queue_enabled,
        )
        .await
    } else {
        remote::invoke(&function, project, payload, headers, url_path).await
    }
}
