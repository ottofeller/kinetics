mod cron;
mod endpoint;
mod worker;

use super::files::BuildFiles;
use kinetics_lib::tools::config::EndpointConfig;
use kinetics_parser::{Params, ParsedFunction};
use std::path::Path;

/// Write local and Lambda entry points for one handler, preserving unchanged files.
pub(super) fn write(
    package_path: &Path,
    function: &ParsedFunction,
    import: &str,
    files: &mut BuildFiles,
) -> eyre::Result<()> {
    for is_local in [false, true] {
        let name = function.func_name(is_local)?;
        let function_name = "__kinetics_user_function";
        let code = match &function.params {
            Params::Endpoint(params) => endpoint::render(
                import,
                function_name,
                EndpointConfig::new(&params.url_path),
                is_local,
            ),
            Params::Worker(_) | Params::DlqWorker(_) => {
                worker::render(import, function_name, is_local)
            }
            Params::Cron(_) => cron::render(import, function_name, is_local),
        };
        let syntax = syn::parse_file(&code)?;
        files.write(
            &package_path.join("src/bin").join(format!("{name}.rs")),
            prettyplease::unparse(&syntax),
        )?;
    }
    Ok(())
}
