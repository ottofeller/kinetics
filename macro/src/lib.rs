#![feature(proc_macro_totokens, proc_macro_span, proc_macro_expand)]
mod common;
use std::fmt::Display;

use proc_macro::TokenStream;

enum FunctionRole {
    Endpoint,
    Worker,
}

impl Display for FunctionRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FunctionRole::Endpoint => write!(f, "endpoint"),
            FunctionRole::Worker => write!(f, "worker"),
        }
    }
}

#[proc_macro_attribute]
pub fn endpoint(attr: TokenStream, item: TokenStream) -> TokenStream {
    common::process_function(attr, item, FunctionRole::Endpoint)
}

#[proc_macro_attribute]
pub fn worker(attr: TokenStream, item: TokenStream) -> TokenStream {
    common::process_function(attr, item, FunctionRole::Worker)
}
