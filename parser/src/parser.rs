use crate::{
    params::{Cron, Endpoint, Params, Worker},
    ParsedFunction, Role,
};
use color_eyre::eyre;
use std::path::{Path, PathBuf};
use syn::{parse::Parse, visit::Visit, Attribute, ItemFn};
use walkdir::WalkDir;

/// For a workspace represents its member.
/// For a standalone crate represents the crate itself.
#[derive(Clone, Debug, Default)]
pub struct Package {
    /// The name of the package as defined in its Cargo.toml
    pub name: String,

    /// Relative path to the package from workspace root
    pub relative_path: PathBuf,
}

#[derive(Debug, Default)]
pub struct Parser<'a> {
    /// All found functions in the source code
    pub functions: Vec<ParsedFunction>,

    /// Relative path to currently processing file
    fn_rel_path: PathBuf,

    /// The package being processed
    pkg: Option<&'a Package>,
}

impl<'a> Parser<'a> {
    /// Init new Parser
    ///
    /// And optionally parse the requested dir
    pub fn new(root_path: &Path, pkg: Option<&'a Package>) -> eyre::Result<Self> {
        let mut parser: Parser = Default::default();

        if let Some(pkg) = pkg {
            parser.set_pkg(Some(pkg));
            parser.walk_dir(&root_path.join(&pkg.relative_path))?;
        }

        Ok(parser)
    }

    pub fn walk_dir(&mut self, path: &Path) -> eyre::Result<()> {
        for entry in WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().strip_prefix(path).is_ok_and(|p| p.starts_with("src/")) // only src folder
                && e.path().extension().is_some_and(|ext| ext == "rs") // only rust files
            })
        {
            let content = std::fs::read_to_string(entry.path())?;
            let syntax = syn::parse_file(&content)?;

            // Set current file relative path for further imports resolution
            // WARN It prevents to implement parallel parsing of files and requires rework in the future
            self.set_fn_rel_path(Some(entry.path().strip_prefix(path)?));

            self.visit_file(&syntax);
        }

        Ok(())
    }

    fn set_pkg(&mut self, pkg: Option<&'a Package>) {
        self.pkg = pkg.to_owned();
    }

    fn set_fn_rel_path(&mut self, file_path: Option<&Path>) {
        self.fn_rel_path = file_path.map(PathBuf::from).unwrap_or_default();
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
    /// Known definitions:
    /// #[kinetics_macro::<role> or <role>]
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

impl<'a> Visit<'_> for Parser<'a> {
    /// Visits function definitions
    fn visit_item_fn(&mut self, item: &ItemFn) {
        for attr in &item.attrs {
            // Skip non-endpoint or non-worker attributes
            let (role, params) = match self.parse_attr_role(attr).as_str() {
                "endpoint" => {
                    let params = self.parse_endpoint(attr).unwrap();
                    (Role::Endpoint, Params::Endpoint(params))
                }
                "worker" => {
                    let params = self.parse_worker(attr).unwrap();
                    (Role::Worker, Params::Worker(params))
                }
                "cron" => {
                    let params = self.parse_cron(attr).unwrap();
                    (Role::Cron, Params::Cron(params))
                }
                _ => continue,
            };

            if let Some(pkg) = self.pkg {
                self.functions.push(ParsedFunction {
                    role,
                    params,
                    rust_function_name: item.sig.ident.to_string(),
                    relative_path: self.fn_rel_path.clone(),
                    pkg_rel_path: pkg.relative_path.clone(),
                    pkg_name: pkg.name.clone(),
                });
            }
        }

        // We don't need to parse the function body (in case nested functions), so just exit here
    }
}
