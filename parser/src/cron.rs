use crate::environment::{parse_environment, Environment};
use syn::{
    parse::{Parse, ParseStream},
    token, Ident, LitStr,
};

#[derive(Debug)]
pub struct Cron {
    pub name: Option<String>,
    pub schedule: String,
    pub environment: Environment,
}

impl Parse for Cron {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut environment = Environment::default();
        let mut schedule = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<token::Eq>()?;

            match ident.to_string().as_str() {
                "name" => {
                    name = Some(input.parse::<LitStr>()?.value());
                }
                "environment" => {
                    environment = parse_environment(input)?;
                }
                "schedule" => {
                    schedule = Some(input.parse::<LitStr>()?.value());
                }
                // Ignore unknown attributes
                _ => {}
            }

            if !input.is_empty() {
                input.parse::<token::Comma>()?;
            }
        }

        if schedule.is_none() {
            return Err(input.error("Cron validation failed: no schedule provided"));
        }

        Ok(Cron {
            name,
            environment,
            schedule: schedule.unwrap(),
        })
    }
}
