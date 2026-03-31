use crate::environment::{parse_environment, Environment};
use serde::{Deserialize, Serialize};
use syn::{
    parse::{Parse, ParseStream},
    token, Ident, LitBool, LitInt, LitStr,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub name: Option<String>,
    pub concurrency: u32,
    pub fifo: bool,
    pub environment: Environment,
    pub batch_size: Option<u32>,
}

impl Parse for Worker {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut concurrency = None;
        let mut fifo = None;
        let mut environment = None;
        let mut batch_size = None;

        while !input.is_empty() {
            let ident_span = input.span();
            let ident: Ident = input.parse()?;
            input.parse::<token::Eq>()?;

            match ident.to_string().as_str() {
                "name" => {
                    if name.is_some() {
                        return Err(syn::Error::new(ident_span, "Duplicate attribute `name`"));
                    }
                    name = Some(input.parse::<LitStr>()?.value());
                }
                "environment" => {
                    if environment.is_some() {
                        return Err(syn::Error::new(
                            ident_span,
                            "Duplicate attribute `environment`",
                        ));
                    }
                    environment = Some(parse_environment(input)?);
                }
                "concurrency" => {
                    if concurrency.is_some() {
                        return Err(syn::Error::new(
                            ident_span,
                            "Duplicate attribute `concurrency`",
                        ));
                    }
                    concurrency = Some(input.parse::<LitInt>()?.base10_parse::<u32>()?);
                }
                "fifo" => {
                    if fifo.is_some() {
                        return Err(syn::Error::new(ident_span, "Duplicate attribute `fifo`"));
                    }
                    fifo = match input.parse::<LitBool>() {
                        Ok(bool) => Some(bool.value),
                        Err(_) => {
                            return Err(input.error("expected boolean value for 'fifo'"));
                        }
                    };
                }
                "batch_size" => {
                    if batch_size.is_some() {
                        return Err(syn::Error::new(
                            ident_span,
                            "Duplicate attribute `batch_size`",
                        ));
                    }

                    let parsed = input.parse::<LitInt>()?.base10_parse::<u32>()?;

                    if parsed == 0 || parsed > 10 {
                        return Err(
                            input.error("Batch size must be a positive integer between 1 and 10")
                        );
                    }

                    batch_size = Some(parsed);
                }
                // Ignore unknown attributes
                _ => {}
            }

            if !input.is_empty() {
                input.parse::<token::Comma>()?;
            }
        }

        Ok(Self {
            name,
            concurrency: concurrency.unwrap_or(1),
            fifo: fifo.unwrap_or_default(),
            environment: environment.unwrap_or_default(),
            batch_size,
        })
    }
}
