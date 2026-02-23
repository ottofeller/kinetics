use crate::environment::{parse_environment, Environment};
use serde::{Deserialize, Serialize};
use syn::{
    parse::{Parse, ParseStream},
    token, Ident, LitStr,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cron {
    pub name: Option<String>,
    pub schedule: String,
    pub environment: Environment,
}

impl Parse for Cron {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut environment = None;
        let mut schedule = None;

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
                "environment" => {
                    if environment.is_some() {
                        return Err(syn::Error::new(
                            ident_span,
                            "Duplicate attribute `environment`",
                        ));
                    }
                    environment = Some(parse_environment(input)?);
                }
                "schedule" => {
                    if schedule.is_some() {
                        return Err(syn::Error::new(
                            ident_span,
                            "Duplicate attribute `schedule`",
                        ));
                    }
                    schedule = Some(input.parse::<LitStr>()?.value());
                }
                // Ignore unknown attributes
                _ => {}
            }

            if !input.is_empty() {
                input.parse::<token::Comma>()?;
            }
        }

        Ok(Cron {
            name,
            environment: environment.unwrap_or_default(),
            schedule: schedule
                .ok_or_else(|| input.error("Cron validation failed: no schedule provided"))?,
        })
    }
}
