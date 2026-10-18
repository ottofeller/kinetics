use crate::environment::{parse_environment, Environment};
use serde::{Deserialize, Serialize};
use syn::{
    parse::{Parse, ParseStream},
    token, Ident, LitInt,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqWorker {
    pub concurrency: u32,
    pub batch_size: Option<u32>,
    pub environment: Environment,
}

impl Parse for DlqWorker {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut concurrency = None;
        let mut environment = None;
        let mut batch_size = None;

        while !input.is_empty() {
            let ident_span = input.span();
            let ident: Ident = input.parse()?;
            input.parse::<token::Eq>()?;

            match ident.to_string().as_str() {
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
                "batch_size" => {
                    if batch_size.is_some() {
                        return Err(syn::Error::new(
                            ident_span,
                            "Duplicate attribute `batch_size`",
                        ));
                    }
                    batch_size = Some(input.parse::<LitInt>()?.base10_parse::<u32>()?);
                }
                // Ignore unknown attributes
                _ => {}
            }

            if !input.is_empty() {
                input.parse::<token::Comma>()?;
            }
        }

        Ok(Self {
            concurrency: concurrency.unwrap_or(2),
            batch_size,
            environment: environment.unwrap_or_default(),
        })
    }
}
