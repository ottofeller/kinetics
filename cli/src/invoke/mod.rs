mod docker;
mod dynamodb;
mod local;
mod remote;
mod sqldb;
use crate::build::prepare_crates;
use crate::config::build_config;
use crate::crat::Crate;
use crate::function::Function;
use std::path::PathBuf;

/// Invoke the function either locally or remotely
pub async fn invoke(
    function_name: &str,
    crat: &Crate,
    payload: &str,
    headers: &str,

    // DynamoDbB table to provision, only relevant for local invocations
    table: Option<&str>,

    is_local: bool,

    is_sqldb_enabled: bool,
) -> eyre::Result<()> {
    // Get function names as well as pull all updates from the code.
    let all_functions = prepare_crates(
        PathBuf::from(build_config()?.kinetics_path),
        crat,
        &[function_name.into()],
    )?;
    let function = Function::find_by_name(&all_functions, function_name)?;

    if is_local {
        local::invoke(&function, crat, payload, headers, table, is_sqldb_enabled).await
    } else {
        remote::invoke(&function, crat, payload, headers).await
    }
}
