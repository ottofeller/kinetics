use std::sync::OnceLock;

#[derive(Debug)]
pub(crate) struct Config<'a> {
    pub(crate) api_base: &'a str,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub(crate) fn build_config() -> &'static Config<'static> {
    CONFIG.get_or_init(|| {
        let api_base =
            option_env!("KINETICS_API_BASE").unwrap_or("https://backend.usekinetics.com/");

        {
            Config { api_base }
        }
    })
}
