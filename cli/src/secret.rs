use rust_dotenv::dotenv::DotEnv;

pub struct Secret {
    name: String,
    value: String,
}

impl Secret {
    pub fn from_dotenv() -> eyre::Result<Vec<Self>> {
        let mut result = vec![];
        let dotenv = DotEnv::new("secrets");

        for (name, value) in dotenv.all_vars() {
            result.push(Secret {
                name: name.clone(),
                value: value.clone(),
            });
        }

        Ok(result)
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
