use crate::environment::{parse_environment, Environment};
use syn::{
    parse::{Parse, ParseStream},
    token, Ident, LitBool, LitInt, LitStr,
};

#[derive(Debug, Clone)]
pub struct Worker {
    pub name: Option<String>,
    pub concurrency: i16,
    pub fifo: bool,
    pub environment: Environment,
}

impl Default for Worker {
    fn default() -> Self {
        Worker {
            name: None,
            concurrency: 1,
            fifo: false,
            environment: Environment::new(),
        }
    }
}

impl Parse for Worker {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut worker = Worker::default();

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<token::Eq>()?;

            match ident.to_string().as_str() {
                "name" => {
                    worker.name = Some(input.parse::<LitStr>()?.value());
                }
                "environment" => {
                    worker.environment = parse_environment(input)?;
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
    }
}
