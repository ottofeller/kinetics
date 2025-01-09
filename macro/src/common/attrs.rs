use crate::FunctionRole;
use eyre::{eyre, WrapErr};
use proc_macro::ToTokens;
use proc_macro::TokenStream;
use serde::Deserialize;
use std::collections::HashMap;
use syn::{LitInt, LitStr};
pub type Environment = HashMap<String, String>;

/// Attributes of the macro
pub enum Attrs {
    Endpoint(EndpointAttrs),
    Worker(WorkerAttrs),
}

#[derive(Deserialize)]
pub struct EndpointAttrs {
    pub name: Option<String>,
    pub url_path: Option<String>,
    pub environment: Option<Environment>,
}

#[derive(Deserialize)]
pub struct WorkerAttrs {
    pub name: Option<String>,
    pub concurrency: Option<i16>,
    pub fifo: Option<bool>,
    pub environment: Option<Environment>,
}

impl Attrs {
    /// Parse the macro input attributes into a hashmap
    pub fn new(input: TokenStream, role: &FunctionRole) -> eyre::Result<Attrs> {
        let mut result = serde_json::json!({});
        let mut name = String::new();

        for token in input.into_iter() {
            match token {
                proc_macro::TokenTree::Ident(ident) => name = ident.to_string(),

                proc_macro::TokenTree::Group(group) => {
                    result[name.clone()] = serde_json::from_str(&group.to_string())
                        .wrap_err("Failed to parse environment")?;
                }

                proc_macro::TokenTree::Literal(literal) => {
                    // Try to parse inptut as a string literal first
                    let token_stream = literal.to_token_stream();
                    let try_str = syn::parse::<LitStr>(token_stream.clone());
                    let try_int = syn::parse::<LitInt>(token_stream);

                    if try_str.is_err() && try_int.is_err() {
                        return Err(eyre!(
                            "The input attr has unsupported format (should be str or int)"
                        ));
                    }

                    if try_int.is_ok() {
                        result[name.clone()] =
                            serde_json::to_value(try_int?.base10_parse::<i16>()?)?;
                    } else {
                        result[name.clone()] = serde_json::to_value(try_str?.value())?;
                    }
                }

                _ => {}
            }
        }

        if let FunctionRole::Worker = role {
            Ok(Attrs::Worker(serde_json::from_value::<WorkerAttrs>(
                result,
            )?))
        } else {
            Ok(Attrs::Endpoint(serde_json::from_value::<EndpointAttrs>(
                result,
            )?))
        }
    }

    /// Function envvars
    pub fn environment(&self) -> Environment {
        match self {
            Attrs::Endpoint(attrs) => attrs.environment.clone(),
            Attrs::Worker(attrs) => attrs.environment.clone(),
        }
        .unwrap_or(Environment::default())
    }

    /// Get the name of the function
    pub fn name(&self) -> Option<String> {
        match self {
            Attrs::Endpoint(attrs) => attrs.name.clone(),
            Attrs::Worker(attrs) => attrs.name.clone(),
        }
    }

    /// Worker specific attributes
    pub fn worker(&self) -> Option<&WorkerAttrs> {
        match self {
            Attrs::Worker(attrs) => Some(attrs),
            _ => None,
        }
    }
}
