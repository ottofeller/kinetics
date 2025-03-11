use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn endpoint(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // TODO: Add attrs validation
    item
}

#[proc_macro_attribute]
pub fn cron(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // TODO: Add attrs validation
    item
}

#[proc_macro_attribute]
pub fn worker(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // TODO: Add attrs validation
    item
}
