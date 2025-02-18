use std::collections::HashMap;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{token, Attribute, Error as SynError, Ident, ItemFn, LitBool, LitInt, LitStr};

type Environment = HashMap<String, String>;

/// Represents a function in the source code
#[derive(Debug)]
pub(crate) struct ParsedFunction {
    /// Name of the function, parsed from the function definition
    pub(crate) rust_function_name: String,

    /// Path to the file where function is defined
    pub(crate) relative_path: String,

    /// Parsed from skymacro macro definition
    pub(crate) role: Role,
}

#[derive(Debug)]
pub(crate) enum Role {
    Endpoint(Endpoint),
    Worker(Worker),
}

impl Role {
    pub fn rust_function_name(&self) -> &str {
        match self {
            Role::Endpoint(params) => &params.name,
            Role::Worker(params) => &params.name,
        }
    }

    pub fn environment(&self) -> &Environment {
        match self {
            Role::Endpoint(params) => &params.environment,
            Role::Worker(params) => &params.environment,
        }
    }
}

#[derive(Default, Debug)]
pub(crate) struct Endpoint {
    pub(crate) name: String,
    pub(crate) url_path: String,
    pub(crate) environment: Environment,
}

#[derive(Debug)]
pub(crate) struct Worker {
    pub(crate) name: String,
    pub(crate) concurrency: i16,
    pub(crate) fifo: bool,
    pub(crate) environment: Environment,
}

impl Default for Worker {
    fn default() -> Self {
        Worker {
            name: "".to_string(),
            concurrency: 1,
            fifo: false,
            environment: Environment::new(),
        }
    }
}

enum ParsedRole {
    Endpoint,
    Worker,
}

impl TryFrom<String> for ParsedRole {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "endpoint" => Ok(ParsedRole::Endpoint),
            "worker" => Ok(ParsedRole::Worker),
            _ => Err("Unknown function type"),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Parser {
    /// All found functions in the source code
    pub(crate) functions: Vec<ParsedFunction>,

    /// Relative path to currently processing file
    relative_path: String,
}

impl Parser {
    pub(crate) fn new() -> Self {
        Self {
            functions: Vec::new(),
            relative_path: String::new(),
        }
    }

    pub(crate) fn set_relative_path(&mut self, file_path: Option<&str>) {
        self.relative_path = file_path.map_or_else(|| "".to_string(), |s| s.to_string());
    }

    fn parse_environment(&mut self, input: ParseStream) -> eyre::Result<Environment, SynError> {
        let content;
        syn::braced!(content in input);
        let vars = Punctuated::<EnvKeyValue, token::Comma>::parse_terminated(&content)?;

        Ok(Environment::from_iter(
            vars.iter().map(|v| (v.key.value(), v.value.value())),
        ))
    }

    fn parse_endpoint(&mut self, attr: &Attribute) -> eyre::Result<Endpoint, SynError> {
        attr.parse_args_with(|input: ParseStream| {
            let mut endpoint = Endpoint::default();

            while !input.is_empty() {
                let ident: Ident = input.parse()?;
                input.parse::<token::Eq>()?;

                match ident.to_string().as_str() {
                    "name" => {
                        endpoint.name = input.parse::<LitStr>()?.value();
                    }
                    "url_path" => {
                        endpoint.url_path = input.parse::<LitStr>()?.value();
                    }
                    "environment" => {
                        endpoint.environment = self.parse_environment(input)?;
                    }
                    // Ignore unknown attributes
                    _ => {}
                }

                if !input.is_empty() {
                    input.parse::<token::Comma>()?;
                }
            }

            Ok(endpoint)
        })
    }

    fn parse_worker(&mut self, attr: &Attribute) -> eyre::Result<Worker, SynError> {
        attr.parse_args_with(|input: ParseStream| {
            let mut worker = Worker::default();

            while !input.is_empty() {
                let ident: Ident = input.parse()?;
                input.parse::<token::Eq>()?;

                match ident.to_string().as_str() {
                    "name" => {
                        worker.name = input.parse::<LitStr>()?.value();
                    }
                    "environment" => {
                        worker.environment = self.parse_environment(input)?;
                    }
                    "concurrency" => {
                        worker.concurrency = input.parse::<LitInt>()?.base10_parse::<i16>()?;
                    }
                    "fifo" => {
                        worker.fifo = match input.parse::<LitBool>() {
                            Ok(bool) => bool.value,
                            Err(_) => {
                                return Err(input.error("expected boolean value for 'fifo'"));
                            }
                        };
                    }
                    // Ignore unknown attributes
                    _ => {}
                }

                if !input.is_empty() {
                    input.parse::<token::Comma>()?;
                }
            }

            Ok(worker)
        })
    }

    /// Checks if the input is a valid skymacro definition
    /// Known definitions:
    /// skymacro::endpoint or endpoint
    /// skymacro::worker or worker
    fn parse_attr(&self, input: &Attribute) -> Option<ParsedRole> {
        let path = input.path();

        if path.segments.len() == 1 {
            let ident = &path.segments[0].ident;
            return ParsedRole::try_from(ident.to_string()).ok();
        }

        if path.segments.len() == 2 && &path.segments[0].ident == "skymacro" {
            let ident = &path.segments[1].ident;
            return ParsedRole::try_from(ident.to_string()).ok();
        }

        None
    }
}

impl<'a> Visit<'a> for Parser {
    /// Visits function definitions
    fn visit_item_fn(&mut self, item: &ItemFn) {
        for attr in &item.attrs {
            // Skip non-endpoint or non-worker attributes
            let role = match self.parse_attr(attr) {
                Some(ParsedRole::Endpoint) => {
                    let params = self.parse_endpoint(attr).unwrap();
                    Role::Endpoint(params)
                }
                Some(ParsedRole::Worker) => {
                    let params = self.parse_worker(attr).unwrap();
                    Role::Worker(params)
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

/// Helper struct to parse environment variables in function attributes
/// It is used to parse individual environment attribute from environment = {"FOO": "BAR", "BAZ": "QUX"}}
/// For example: "FOO": "BAR" becomes EnvKeyValue { key: "FOO", value: "BAR" }
struct EnvKeyValue {
    key: LitStr,
    value: LitStr,
}

impl Parse for EnvKeyValue {
    fn parse(input: ParseStream) -> eyre::Result<Self, SynError> {
        let key: LitStr = input.parse()?;
        input.parse::<token::Colon>()?;
        let value: LitStr = input.parse()?;
        Ok(EnvKeyValue { key, value })
    }
}
