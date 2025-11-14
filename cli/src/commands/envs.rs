use super::build::prepare_functions;
use crate::config::build_config;
use crate::project::Project;
use crossterm::style::Stylize;
use eyre::eyre;
use kinetics_parser::Parser;
use std::path::PathBuf;

/// Lists all environment variables for all functions in the current crate
pub async fn list(project: &Project) -> eyre::Result<()> {
    let functions = prepare_functions(PathBuf::from(build_config()?.kinetics_path), project, &[])?;
    let parsed_functions = Parser::new(Some(&project.path))?.functions;

    if functions.is_empty() {
        println!("{}", "No functions found".yellow());
        return Ok(());
    }

    for function in functions {
        let path = parsed_functions
            .iter()
            .find(|f| {
                f.func_name(false)
                    .inspect_err(|e| log::error!("Failed to get function name: {e:?}"))
                    .unwrap()
                    == function.name
            })
            .ok_or(eyre!("Parsed artifact has no function name"))
            .inspect_err(|e| log::error!("{e:?}"))?
            .relative_path
            .to_owned();

        let function_name = function.clone().name;

        // Get environment variables for this function
        let env_vars = function.environment();

        if env_vars.is_empty() {
            continue;
        }

        println!(
            "{} {}",
            function_name.bold(),
            format!("from {}", path).dim()
        );

        for (key, value) in env_vars.clone() {
            println!("{} {}", key.dim(), value.black());
        }

        println!();
    }

    Ok(())
}
