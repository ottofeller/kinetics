use super::database::AnalysisDatabase;
use super::emission::EmissionPlan;
use super::reachability::Reachability;
use eyre::{eyre, ContextCompat};
use kinetics_parser::ParsedFunction;
use ra_ap_hir::{attach_db, Function, Semantics};
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{ast, AstNode};
use std::path::Path;

/// Orchestrates entrypoint resolution, semantic reachability, and source emission.
#[derive(Debug)]
pub(crate) struct TreeShaker {
    /// Loaded rust-analyzer database for one package.
    database: AnalysisDatabase,
}

impl TreeShaker {
    /// Loads a package and initializes its semantic source database.
    pub(crate) fn load(package_root: &Path) -> eyre::Result<Self> {
        Ok(Self {
            database: AnalysisDatabase::load(package_root)?,
        })
    }

    /// Computes how source files should be emitted for one parsed Kinetics function.
    pub(crate) fn emission_plan(
        &self,
        parsed_function: &ParsedFunction,
    ) -> eyre::Result<EmissionPlan> {
        let function = self.find_function(parsed_function)?;
        let reachability = Reachability::new(&self.database).build(function);
        let plan = EmissionPlan::build(
            &self.database,
            function,
            &parsed_function.rust_function_name,
            reachability,
        );
        plan.log_summary(&parsed_function.rust_function_name);
        Ok(plan)
    }

    /// Resolves the parser result through its physical source file and AST function definition.
    fn find_function(&self, parsed_function: &ParsedFunction) -> eyre::Result<Function> {
        let source_path = self
            .database
            .package_root
            .join(&parsed_function.relative_path);
        let file_id = self
            .database
            .source_index
            .file_id(&source_path)
            .wrap_err_with(|| format!("Function source {source_path:?} is not registered"))?;
        let semantics = Semantics::new_dyn(self.database.host.raw_database());
        let mut matches = Vec::new();

        attach_db(semantics.db, || {
            let source_file = semantics.parse_guess_edition(file_id);
            for function in source_file.syntax().descendants().filter_map(ast::Fn::cast) {
                if function
                    .name()
                    .is_some_and(|name| name.text() == parsed_function.rust_function_name)
                {
                    if let Some(function) = semantics.to_fn_def(&function) {
                        matches.push(function);
                    }
                }
            }
        });

        match matches.as_slice() {
            [function] => Ok(*function),
            [] => Err(eyre!(
                "Function {:?} was not found in {:?}",
                parsed_function.rust_function_name,
                parsed_function.relative_path
            )),
            _ => Err(eyre!(
                "Function {:?} is ambiguous in {:?}",
                parsed_function.rust_function_name,
                parsed_function.relative_path
            )),
        }
    }
}
