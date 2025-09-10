use std::path::PathBuf;

use crate::{environment::Environment, Cron, Endpoint, Worker};
use color_eyre::eyre;
use syn::{parse::Parse, visit::Visit, Attribute, ItemFn};
use walkdir::WalkDir;

/// Represents a function in the source code
#[derive(Debug, Clone)]
pub struct ParsedFunction {
    /// Name of the function, parsed from the function definition
    pub rust_function_name: String,

    /// Path to the file where function is defined
    pub relative_path: String,

    /// Parsed from kinetics_macro macro definition
    pub role: Role,
}

impl ParsedFunction {
    /// Convert a path to CamelCase name
    pub fn path_to_name(path: &str) -> String {
        return path
            .split(&['.', '/'])
            .filter(|s| !s.eq(&"rs"))
            .map(|s| match s.chars().next() {
                Some(first) => first.to_uppercase().collect::<String>() + &s[1..],
                None => String::new(),
            })
            .collect::<String>()
            .replacen("Src", "", 1);
    }

    /// Generate lambda function name out of Rust function name or macro attribute
    ///
    /// By default use the Rust function plus crate path as the function name. Convert
    /// some-name to SomeName, and do other transformations in order to comply with Lambda
    /// function name requirements.
    pub fn func_name(&self, is_local: bool) -> eyre::Result<String> {
        let rust_name = &self.rust_function_name;
        let full_path = format!("{}/{rust_name}", self.relative_path);
        let default_func_name = Self::path_to_name(&full_path);
        let name = self.role.name().unwrap_or(&default_func_name);

        if name.len() > 64 {
            Err(eyre::eyre!(
                "Function name is longer than 64 chars: {}",
                name
            ))
        } else {
            // TODO Check the name for uniqueness
            Ok(format!("{}{}", name, if is_local { "Local" } else { "" }))
        }
    }
}

#[derive(Debug, Clone)]
pub enum Role {
    Endpoint(Endpoint),
    Cron(Cron),
    Worker(Worker),
}

impl Role {
    pub fn name(&self) -> Option<&String> {
        match self {
            Role::Endpoint(params) => params.name.as_ref(),
            Role::Cron(params) => params.name.as_ref(),
            Role::Worker(params) => params.name.as_ref(),
        }
    }

    pub fn environment(&self) -> &Environment {
        match self {
            Role::Endpoint(params) => &params.environment,
            Role::Cron(params) => &params.environment,
            Role::Worker(params) => &params.environment,
        }
    }
}

#[derive(Debug, Default)]
pub struct Parser {
    /// All found functions in the source code
    pub functions: Vec<ParsedFunction>,

    /// Relative path to currently processing file
    pub relative_path: String,
}

impl Parser {
    /// Init new Parser
    ///
    /// And optionally parse the requested dir
    pub fn new(path: Option<&PathBuf>) -> eyre::Result<Self> {
        let mut parser: Parser = Default::default();

        if path.is_some() {
            parser.walk_dir(path.unwrap())?;
        }

        Ok(parser)
    }

    pub fn walk_dir(&mut self, path: &PathBuf) -> eyre::Result<()> {
        for entry in WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
        {
            let content = std::fs::read_to_string(entry.path())?;
            let syntax = syn::parse_file(&content)?;

            // Set current file relative path for further imports resolution
            // WARN It prevents to implement parallel parsing of files and requires rework in the future
            self.set_relative_path(entry.path().strip_prefix(path)?.to_str());

            self.visit_file(&syntax);
        }

        Ok(())
    }

    pub fn set_relative_path(&mut self, file_path: Option<&str>) {
        self.relative_path = file_path.map_or_else(|| "".to_string(), |s| s.to_string());
    }

    fn parse_endpoint(&mut self, attr: &Attribute) -> syn::Result<Endpoint> {
        attr.parse_args_with(Endpoint::parse)
    }

    fn parse_worker(&mut self, attr: &Attribute) -> syn::Result<Worker> {
        attr.parse_args_with(Worker::parse)
    }

    fn parse_cron(&mut self, attr: &Attribute) -> syn::Result<Cron> {
        attr.parse_args_with(Cron::parse)
    }

    /// Checks if the input is a valid kinetics_macro definition and returns its role
    /// Checks if the input is a valid kinetics_macro definition
    /// Known definitions:
    /// kinetics_macro::<role> or <role>
    fn parse_attr_role(&self, input: &Attribute) -> String {
        let path = input.path();

        if path.segments.len() == 1 {
            let ident = &path.segments[0].ident;
            return ident.to_string();
        }

        if path.segments.len() == 2 && &path.segments[0].ident == "kinetics_macro" {
            let ident = &path.segments[1].ident;
            return ident.to_string();
        }

        "".to_string()
    }
}

impl Visit<'_> for Parser {
    /// Visits function definitions
    fn visit_item_fn(&mut self, item: &ItemFn) {
        for attr in &item.attrs {
            // Skip non-endpoint or non-worker attributes
            let role = match self.parse_attr_role(attr).as_str() {
                "endpoint" => {
                    let params = self.parse_endpoint(attr).unwrap();
                    Role::Endpoint(params)
                }
                "worker" => {
                    let params = self.parse_worker(attr).unwrap();
                    Role::Worker(params)
                }
                "cron" => {
                    let params = self.parse_cron(attr).unwrap();
                    Role::Cron(params)
                }
                _ => continue,
            };

            self.functions.push(ParsedFunction {
                role,
                rust_function_name: item.sig.ident.to_string(),
                relative_path: self.relative_path.clone(),
            });
        }

        // We don't need to parse the function body (in case nested functions), so just exit here
    }
}
