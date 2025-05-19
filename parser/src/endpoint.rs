use crate::environment::{parse_environment, Environment};
use syn::{
    parse::{Parse, ParseStream},
    token, Ident, LitStr,
};

#[derive(Default, Debug, Clone)]
pub struct Endpoint {
    pub name: Option<String>,
    pub url_path: Option<String>,
    pub environment: Environment,
    pub queues: Option<Vec<String>>,
}

impl Parse for Endpoint {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut url_path = None;
        let mut environment = Environment::default();
        let mut queues = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<token::Eq>()?;

            match ident.to_string().as_str() {
                "name" => {
                    name = Some(input.parse::<LitStr>()?.value());
                }
                "url_path" => {
                    url_path = Some(input.parse::<LitStr>()?.value());
                }
                "environment" => {
                    environment = parse_environment(input)?;
                }
                "queues" => {
                    let content;
                    syn::bracketed!(content in input);
                    let queue_list = content.parse::<LitStr>()?.value();

                    queues = Some(
                        queue_list
                            // Remove square brackets
                            .trim_matches(|c| c == '[' || c == ']')
                            .split(',')
                            // Remove whitespaces and quotes per item
                            .map(|i| i.trim().trim_matches('"').to_string())
                            .collect::<Vec<String>>(),
                    );
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
            url_path,
            environment,
            queues,
        })
    }
}
