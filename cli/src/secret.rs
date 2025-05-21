use rust_dotenv::dotenv::DotEnv;

pub struct Secret {
    pub name: String,
    value: String,
}

impl Secret {
    /// Read secrets from the .env file
    pub fn from_dotenv() -> Vec<Self> {
        // Return early if the .env.secrets file doesn't exist,
        // to avoid rust_dotenv throwing error message
        if !std::path::Path::new(".env.secrets").exists() {
            log::warn!("No .env.secrets file found");
            return vec![];
        }

        let mut result = vec![];
        let dotenv = DotEnv::new("secrets");

        for (name, value) in dotenv.all_vars() {
            result.push(Secret {
                name: name.clone(),
                value: value.clone(),
            });
        }

        result
    }

    pub fn value(&self) -> String {
        self.value.clone()
    }
}
