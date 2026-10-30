use super::Project;
use crate::function::Function;
use kinetics_parser::{ParsedFunction, Parser};

impl Project {
    pub fn functions(&self) -> eyre::Result<Vec<Function>> {
        self.parsed_functions()?
            .iter()
            .map(|function| Function::new(self, function))
            .collect()
    }

    pub fn parsed_functions(&self) -> eyre::Result<Vec<ParsedFunction>> {
        self.workspace
            .packages
            .iter()
            .filter(|pkg| self.is_in_project(pkg))
            .map(|pkg| Parser::new(&self.workspace.root_path, Some(pkg)))
            .try_fold(Vec::new(), |mut acc, parser| {
                acc.append(&mut parser?.functions);
                Ok(acc)
            })
    }
}
