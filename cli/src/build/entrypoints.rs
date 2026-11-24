use super::files::BuildFiles;
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
        let mode = if is_local { "local" } else { "remote" };
        let code = match &function.params {
            Params::Endpoint(params) => format!(
                "{import}\nkinetics_lib::endpoint_main!({mode}, {function_name}, {:?});",
                params.url_path,
            ),
            Params::Worker(_) | Params::DlqWorker(_) => {
                format!("{import}\nkinetics_lib::worker_main!({mode}, {function_name});")
            }
            Params::Cron(_) => {
                format!("{import}\nkinetics_lib::cron_main!({mode}, {function_name});")
            }
        };
        let syntax = syn::parse_file(&code)?;
        files.write(
            &package_path.join("src/bin").join(format!("{name}.rs")),
            prettyplease::unparse(&syntax),
        )?;
    }
    Ok(())
}
