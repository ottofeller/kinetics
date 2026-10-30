use proc_macro::TokenStream;

// Keep workload markers compilable without pulling in the Lambda runtime.
#[proc_macro_attribute]
pub fn endpoint(_: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn worker(_: TokenStream, item: TokenStream) -> TokenStream {
    item
}
