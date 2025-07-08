mod dynamodb;
mod local;
use crate::crat::Crate;
use crate::function::Function;

/// Invoke the function either locally or remotely
pub async fn invoke(
    function: &Function,
    crat: &Crate,
    payload: &str,
    headers: &str,

    // DynamoDbB table to provision, only relevant for local invocations
    table: Option<&str>,

    is_local: bool,
) -> eyre::Result<()> {
    if is_local {
        return local::invoke(function, crat, payload, headers, table).await;
    }

    Ok(())
}
