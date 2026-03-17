use crate::environment::{parse_environment, Environment};
use http::Method;
use serde::{Deserialize, Serialize};
use syn::{
    bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token, Ident, LitBool, LitStr,
};

const ALLOWED_METHODS: [Method; 5] = [
    Method::GET,
    Method::POST,
    Method::PUT,
    Method::DELETE,
    Method::PATCH,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub name: Option<String>,
    pub url_path: String,
    pub environment: Environment,
    pub is_disabled: Option<bool>,
    pub methods: Vec<String>,
}

impl Parse for Endpoint {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut url_path = None;
        let mut environment = None;
        let mut is_disabled = None;
        let mut methods = vec![];

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
                "url_path" => {
                    if url_path.is_some() {
                        return Err(syn::Error::new(
                            ident_span,
                            "Duplicate attribute `url_path`",
                        ));
                    }
                    url_path = Some(input.parse::<LitStr>()?.value());
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
                "is_disabled" => {
                    if is_disabled.is_some() {
                        return Err(syn::Error::new(
                            ident_span,
                            "Duplicate attribute `is_disabled`",
                        ));
                    }
                    is_disabled = Some(input.parse::<LitBool>()?.value());
                }
                "methods" => {
                    if !methods.is_empty() {
                        return Err(syn::Error::new(ident_span, "Duplicate attribute `methods`"));
                    }

                    // Parse a list of methods with a comma delimiter, like: ["POST", "GET", ...]
                    let content;
                    bracketed!(content in input);
                    let parsed = Punctuated::<LitStr, token::Comma>::parse_terminated(&content)?;

                    methods = parsed
                        .iter()
                        .map(|item| LitStr::value(item).to_uppercase())
                        .collect();

                    for method in methods.iter() {
                        match Method::from_bytes(method.as_bytes()) {
                            Ok(method) => {
                                if !ALLOWED_METHODS.contains(&method) {
                                    let allowed = ALLOWED_METHODS
                                        .iter()
                                        .map(|m| m.as_str())
                                        .collect::<Vec<&str>>()
                                        .join(", ");

                                    return Err(syn::Error::new(
                                        ident_span,
                                        format!(
                                            "Unsupported method: {method}. Available: [{allowed}]",
                                        ),
                                    ));
                                }
                            }
                            Err(err) => {
                                return Err(syn::Error::new(
                                    ident_span,
                                    format!("Invalid method: {}", err),
                                ))
                            }
                        }
                    }
                }

                // Ignore unknown attributes
                _ => {}
            }

            if !input.is_empty() {
                input.parse::<token::Comma>()?;
            }
        }

        Ok(Endpoint {
            name,
            url_path: url_path
                .ok_or_else(|| input.error("Missing required attribute `url_path`"))?,
            environment: environment.unwrap_or_default(),
            methods,
            is_disabled,
        })
    }
}
