// Re-export plugin functions
pub use self::implementation::*;

#[cfg(feature = "enable-direct-deploy")]
pub mod implementation {
    // Re-export optional backend modules for direct deploy feature
    use crate::config::build_config;
    use crate::function::Function;
    use aws_config::BehaviorVersion;
    use aws_sdk_s3::Client;
    use common::template;
    use eyre::Context;
    use std::collections::HashMap;

    pub async fn deploy_directly(
        toml_string: String,
        secrets: HashMap<String, String>,
        functions: &[Function],
    ) -> eyre::Result<()> {
        // WARN This will be moved into kinetics-backend crate to hide details of backend implementation
        let build_config = build_config();
        let crat = template::Crate::new(&toml_string).wrap_err("Invalid crate toml")?;

        let secrets = secrets
            .iter()
            .map(|(k, v)| {
                template::Secret::new(k, v, &crat.name_escaped, build_config.username_escaped)
            })
            .collect::<Vec<template::Secret>>();

        let template = template::Template::new(
            &crat,
            functions
                .iter()
                .map(|f| {
                    template::Function::new(
                        &f.toml_string().unwrap(),
                        &f.s3key_encrypted.to_owned().unwrap(),
                        "",
                        false,
                    )
                    .unwrap()
                })
                .collect::<Vec<template::Function>>(),
            &secrets,
            build_config.s3_bucket_name,
            build_config.username_escaped,
            build_config.username,
            build_config.cloud_front_domain,
            build_config.hosted_zone_id,
            build_config.kms_key_id,
            build_config.lambda_credentials_role_arn,
        )
        .await?;

        for secret in secrets.iter() {
            secret.sync().await?;
        }

        template
            .provision()
            .await
            .wrap_err("Failed to provision template")?;

        Ok(())
    }

    pub async fn upload(function: &mut Function) -> eyre::Result<()> {
        // WARN This will be moved into kinetics-backend to hide details of backend implementation
        // Upload the backend manually if the /upload endpoint gets deleted accidentally

        let body = function.zip_stream().await?;
        let config = aws_config::defaults(BehaviorVersion::v2025_01_17())
            .load()
            .await;

        let client = Client::new(&config);
        let direct_s3key = uuid::Uuid::new_v4().to_string();
        function.set_s3key_encrypted(direct_s3key.clone());

        client
            .put_object()
            .bucket(build_config().s3_bucket_name)
            .key(direct_s3key)
            .body(body)
            .send()
            .await
            .wrap_err("Failed to upload file to S3")?;

        Ok(())
    }
}

#[cfg(not(feature = "enable-direct-deploy"))]
pub mod implementation {
    use crate::function::Function;
    use std::collections::HashMap;

    pub async fn deploy_directly(
        _toml_string: String,
        _secrets: HashMap<String, String>,
        _functions: &[Function],
    ) -> eyre::Result<()> {
        unreachable!()
    }

    pub async fn upload(_function: &mut Function) -> eyre::Result<()> {
        unreachable!()
    }
}
