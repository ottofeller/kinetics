mod dynamodb;
mod local;
mod remote;
use std::path::PathBuf;

use crate::build::prepare_crates;
use crate::config::build_config;
use crate::crat::Crate;
use crate::function::Function;

/// Invoke the function either locally or remotely
pub async fn invoke(
    function_name: &str,
    crat: &Crate,
    payload: &str,
    headers: &str,

    // DynamoDbB table to provision, only relevant for local invocations
    table: Option<&str>,

    is_local: bool,
) -> eyre::Result<()> {
    // Get function names as well as pull all updates from the code.
    let all_functions = prepare_crates(PathBuf::from(build_config()?.build_path), crat)?;
    let function = Function::find_by_name(&all_functions, function_name)?;

    if is_local {
        local::invoke(&function, crat, payload, headers, table).await
    } else {
        remote::invoke(&function, crat, payload, headers).await
    }
}
