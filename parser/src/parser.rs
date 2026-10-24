use crate::{
    params::{Cron, DlqWorker, Endpoint, Params, Worker},
    ParsedFunction, Role,
};
use color_eyre::eyre;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use syn::{parse::Parse, visit::Visit, Attribute, ItemFn, Meta};
use walkdir::WalkDir;

/// For a workspace represents its member.
/// For a standalone crate represents the crate itself.
#[derive(Clone, Debug, Default)]
pub struct Package {
    /// Name from Cargo.toml
    pub name: String,

    /// Relative path to the package from workspace root
    pub relative_path: PathBuf,
}

/// Source files and workload metadata from a single read of a package.
pub struct ParsedPackage {
    pub sources: BTreeMap<PathBuf, syn::File>,
    pub functions: Vec<ParsedFunction>,
}

impl ParsedPackage {
    /// Read syntax for dependency analysis, optionally discovering package workloads.
    pub fn read(
        directory: &Path,
        lib_path: &Path,
        package: Option<&Package>,
    ) -> eyre::Result<Self> {
        let mut sources = read_sources(directory)?;
        if !sources.contains_key(lib_path) {
            let path = directory.join(lib_path);
            let content = if path.exists() {
                fs::read_to_string(path)?
            } else {
                String::new()
            };
            sources.insert(lib_path.to_path_buf(), syn::parse_file(&content)?);
        }
        let mut parser = Parser {
            pkg: package,
            ..Default::default()
        };
        if package.is_some() {
            parser.parse_sources(&sources)?;
        }
        Ok(Self {
            sources,
            functions: parser.functions,
        })
    }
}

fn read_sources(directory: &Path) -> eyre::Result<BTreeMap<PathBuf, syn::File>> {
    let mut sources = BTreeMap::new();
    let src = directory.join("src");
    if !src.exists() {
        return Ok(sources);
    }
    for entry in WalkDir::new(src) {
        let entry = entry?;
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "rs") {
            sources.insert(
                entry.path().strip_prefix(directory)?.to_path_buf(),
                syn::parse_file(&fs::read_to_string(entry.path())?)?,
            );
        }
    }
    Ok(sources)
}

#[derive(Debug, Default)]
pub struct Parser<'a> {
    /// All found functions in the source code
    pub functions: Vec<ParsedFunction>,

    /// Relative path to currently processing file
    fn_rel_path: PathBuf,

    /// The package being processed
    pkg: Option<&'a Package>,

    /// Attribute parse errors, collected during the project walk
    errors: Vec<String>,
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
        self.parse_sources(&read_sources(path)?)
    }

    fn parse_sources(&mut self, sources: &BTreeMap<PathBuf, syn::File>) -> eyre::Result<()> {
        for (path, syntax) in sources {
            self.set_fn_rel_path(Some(path));
            self.visit_file(syntax);
        }

        self.functions.sort_by(|left, right| {
            left.relative_path
                .cmp(&right.relative_path)
                .then_with(|| left.rust_function_name.cmp(&right.rust_function_name))
        });

        if !self.errors.is_empty() {
            eyre::bail!(
                "Invalid kinetics macro attributes:\n{}",
                self.errors.join("\n")
            );
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
        parse_attr_args(attr)
    }

    fn parse_worker(&mut self, attr: &Attribute) -> syn::Result<Worker> {
        parse_attr_args(attr)
    }

    fn parse_cron(&mut self, attr: &Attribute) -> syn::Result<Cron> {
        parse_attr_args(attr)
    }

    /// Remember an attribute parse error with the file
    /// and attribute it came from.
    /// All errors are reported together after the walk.
    fn record_attr_error(&mut self, attr: &Attribute, error: syn::Error) {
        let path = attr
            .path()
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");

        self.errors.push(format!(
            "{}: #[{}] {}",
            self.fn_rel_path.to_string_lossy(),
            path,
            error
        ));
    }

    fn parse_dlq_worker(&mut self, attr: &Attribute) -> syn::Result<DlqWorker> {
        parse_attr_args(attr)
    }

    /// Checks if the input is a valid kinetics_macro definition and returns its role
    /// Known definitions:
    /// #[<role>, kinetics_macro::<role> or kinetics_lib::macros::<role>]
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

        if path.segments.len() == 3
            && &path.segments[0].ident == "kinetics_lib"
            && &path.segments[1].ident == "macros"
        {
            let ident = &path.segments[2].ident;
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
                "endpoint" => match self.parse_endpoint(attr) {
                    Ok(params) => (Role::Endpoint, Params::Endpoint(params)),
                    Err(error) => {
                        self.record_attr_error(attr, error);
                        continue;
                    }
                },
                "worker" => match self.parse_worker(attr) {
                    Ok(params) => (Role::Worker, Params::Worker(params)),
                    Err(error) => {
                        self.record_attr_error(attr, error);
                        continue;
                    }
                },
                "dlq_worker" => match self.parse_dlq_worker(attr) {
                    Ok(params) => (Role::DlqWorker, Params::DlqWorker(params)),
                    Err(error) => {
                        self.record_attr_error(attr, error);
                        continue;
                    }
                },
                "cron" => match self.parse_cron(attr) {
                    Ok(params) => (Role::Cron, Params::Cron(params)),
                    Err(error) => {
                        self.record_attr_error(attr, error);
                        continue;
                    }
                },
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

/// Parse the arguments similar to proc macro:
/// - `#[worker(fifo = true)]` passes the tokens inside the parentheses;
/// - bare `#[worker]` is treated as empty arguments
///   and yields default parameters.
fn parse_attr_args<T: Parse>(attr: &Attribute) -> syn::Result<T> {
    match &attr.meta {
        Meta::Path(_) => syn::parse2(Default::default()),
        Meta::List(list) => syn::parse2(list.tokens.clone()),
        Meta::NameValue(name_value) => Err(syn::Error::new(
            name_value.eq_token.span,
            "expected parentheses with parameters, e.g. `#[worker(...)]`",
        )),
    }
}
