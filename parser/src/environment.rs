use std::collections::HashMap;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token, LitStr,
};

pub(crate) type Environment = HashMap<String, String>;

/// Helper struct to parse environment variables in function attributes
/// It is used to parse individual environment attribute from environment = {"FOO": "BAR", "BAZ": "QUX"}}
/// For example: "FOO": "BAR" becomes EnvKeyValue { key: "FOO", value: "BAR" }
pub(crate) struct EnvKeyValue {
    key: LitStr,
    value: LitStr,
}

impl Parse for EnvKeyValue {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key: LitStr = input.parse()?;
        input.parse::<token::Colon>()?;
        let value: LitStr = input.parse()?;
        Ok(EnvKeyValue { key, value })
    }
}

pub(crate) fn parse_environment(input: ParseStream) -> syn::Result<Environment> {
    let content;
    syn::braced!(content in input);
    let vars = Punctuated::<EnvKeyValue, token::Comma>::parse_terminated(&content)?;

    Ok(Environment::from_iter(
        vars.iter().map(|v| (v.key.value(), v.value.value())),
    ))
}
