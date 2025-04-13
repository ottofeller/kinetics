use crate::template::Crate;
use eyre::Context;

#[derive(Clone, Debug)]
pub struct Secret {
    name: String,
    value: String,
    unique: String,
}

impl Secret {
    pub fn new(name: &str, value: &str, crat: &Crate, username: &str) -> Self {
        Secret {
            name: name.to_string(),
            value: value.to_string(),
            unique: format!("{username}-{crate_name}", crate_name = crat.name_escaped),
        }
    }

    pub fn unique_name(&self) -> String {
        format!("{}-{}", self.unique, self.name)
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

        // Add tags
        client
            .add_tags_to_resource()
            .resource_type(aws_sdk_ssm::types::ResourceTypeForTagging::Parameter)
            .resource_id(self.unique_name())
            .tags(
                aws_sdk_ssm::types::Tag::builder()
                    .key("original_name")
                    .value(&self.name)
                    .build()
                    .wrap_err("Failed to build AWS tag")?,
            )
            .send()
            .await
            .wrap_err("Failed to add tags to SSM param")?;

        Ok(())
    }
}
