use kinetics_parser::{Cron, Endpoint, Worker};
use proc_macro::TokenStream;
use syn::parse_macro_input;

#[proc_macro_attribute]
pub fn endpoint(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the macro attributes in order to validate the inputs,
    // then discard the result.
    let _args = parse_macro_input!(attr as Endpoint);
    item
}

#[proc_macro_attribute]
pub fn cron(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the macro attributes in order to validate the inputs,
    // then discard the result.
    let _args = parse_macro_input!(attr as Cron);
    item
}

#[proc_macro_attribute]
pub fn worker(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the macro attributes in order to validate the inputs,
    // then discard the result.s
    let _args = parse_macro_input!(attr as Worker);
    item
}
