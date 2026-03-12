use crate::environment::{parse_environment, Environment};
use serde::{Deserialize, Serialize};
use syn::{
    bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token, Ident, LitBool, LitStr,
};

const ALLOWED_METHODS: [&str; 6] = ["GET", "POST", "PUT", "PATCH", "DELETE", "ANY"];

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

                    let content;
                    bracketed!(content in input);
                    let parsed = Punctuated::<LitStr, token::Comma>::parse_terminated(&content)?;

                    methods = parsed
                        .iter()
                        .map(|item| LitStr::value(item).to_uppercase())
                        .collect();

                    if methods.contains(&"ANY".to_owned()) && methods.len() != 1 {
                        return Err(syn::Error::new(
                            ident_span,
                            "Mixing `ANY` with other methods is not allowed",
                        ));
                    }

                    for method in methods.iter() {
                        if !ALLOWED_METHODS.contains(&method.as_str()) {
                            return Err(syn::Error::new(
                                ident_span,
                                format!("Invalid method: {}", method),
                            ));
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

        // Use ANY method as a fallback if no methods are specified
        if methods.is_empty() {
            methods.push("ANY".to_string());
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
