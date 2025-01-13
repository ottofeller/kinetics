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

    async fn is_exists(&self, client: &aws_sdk_secretsmanager::Client) -> eyre::Result<bool> {
        let result = client
            .describe_secret()
            .set_secret_id(Some(self.name.clone()))
            .send()
            .await;

        if let Err(e) = &result {
            if let aws_sdk_cloudformation::error::SdkError::ServiceError(err) = e {
                if err.err().meta().code().unwrap().eq("ResourceNotFoundException") {
                    return Ok(false);
                } else {
                    return Err(eyre::eyre!(
                        "Service error while describing secret: {:?}",
                        err
                    ));
                }
            } else {
                return Err(eyre::eyre!("Failed to describe secret: {:?}", e));
            }
        }

        Ok(true)
    }

    pub async fn sync(&self) -> eyre::Result<()> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = aws_sdk_secretsmanager::Client::new(&config);
        let secret_name = self.name.clone();
        let secret_value = self.value.clone();

        if self.is_exists(&client).await? {
            client
                .update_secret()
                .secret_id(secret_name)
                .secret_string(secret_value.clone())
                .send()
                .await?;
        } else {
            client
                .create_secret()
                .name(secret_name)
                .secret_string(secret_value.clone())
                .send()
                .await?;
        }

        Ok(())
    }
}
