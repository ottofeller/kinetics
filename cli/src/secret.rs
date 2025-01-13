use eyre::Context;
use rust_dotenv::dotenv::DotEnv;

pub struct Secret {
    name: String,
    value: String,
    unique: String,
}

impl Secret {
    /// Read secrets from the .env file
    ///
    /// # Arguments
    ///
    /// * `unique` - Configurable unique part of the name
    pub fn from_dotenv(unique: &str) -> eyre::Result<Vec<Self>> {
        let mut result = vec![];
        let dotenv = DotEnv::new("secrets");

        for (name, value) in dotenv.all_vars() {
            result.push(Secret {
                name: name.clone(),
                value: value.clone(),
                unique: unique.to_string(),
            });
        }

        Ok(result)
    }

    pub fn unique_name(&self) -> String {
        format!("{}-{}", self.unique.clone(), self.name.clone())
    }

    pub async fn sync(&self) -> eyre::Result<()> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = aws_sdk_ssm::Client::new(&config);

        client
            .put_parameter()
            .overwrite(true)
            .r#type(aws_sdk_ssm::types::ParameterType::SecureString)
            .name(self.unique_name())
            .value(self.value.clone())
            .send()
            .await
            .wrap_err("Failed to create SSM param")?;

        Ok(())
    }
}
