pub mod deploy;
pub mod status;

#[cfg(feature = "enable-direct-deploy")]
pub mod implementation {
    // Re-export optional backend modules for direct deploy feature
    pub use ::backend::*;

    pub async fn deploy_directly() -> eyre::Result<()> {
        // WARN This will be moved into kinetics-backend
        use crate::config::backend::template::{
            Crate as BackendCrate, Function as BackendFunction,
        };
        use crate::config::build_config;

        let build_config = build_config();
        let crat = BackendCrate::new(self.toml_string.clone()).wrap_err("Invalid crate toml")?;

        let secrets = secrets
            .iter()
            .map(|(k, v)| {
                backend::template::Secret::new(k, v, &crat, build_config.username_escaped)
            })
            .collect::<Vec<backend::template::Secret>>();

        let template = backend::template::Template::new(
            &crat,
            functions
                .iter()
                .map(|f| {
                    BackendFunction::new(
                        &f.toml_string().unwrap(),
                        &f.s3key_encrypted.to_owned().unwrap(),
                        "",
                        false,
                    )
                    .unwrap()
                })
                .collect::<Vec<BackendFunction>>(),
            secrets.clone(),
            build_config.s3_bucket_name,
            build_config.username_escaped,
            build_config.username,
            build_config.cloud_front_domain,
        )
        .await?;

        for secret in secrets.iter() {
            secret.sync().await?;
        }

        template
            .provision()
            .await
            .wrap_err("Failed to provision template")?;

        return Ok(());
    }

    pub async fn upload() -> eyre::Result<()> {
        // WARN This will be moved into kinetics-backend
        // Upload the backend manually if the /upload endpoint gets deleted accidentally
        use crate::config::build_config;
        use aws_config::BehaviorVersion;
        use aws_sdk_s3::Client;

        let body = self.zip_stream().await?;
        let config = aws_config::defaults(BehaviorVersion::v2025_01_17())
            .load()
            .await;

        let client = Client::new(&config);
        let direct_s3key = uuid::Uuid::new_v4().to_string();
        self.set_s3key_encrypted(direct_s3key.clone());

        client
            .put_object()
            .bucket(build_config().s3_bucket_name)
            .key(direct_s3key)
            .body(body)
            .send()
            .await
            .wrap_err("Failed to upload file to S3")?;

        return Ok(());
    }
}

#[cfg(not(feature = "enable-direct-deploy"))]
pub mod implementation {
    pub async fn deploy_directly() -> eyre::Result<()> {
        Ok(())
    }

    pub async fn upload() -> eyre::Result<()> {
        Ok(())
    }
}

pub use self::implementation::*;
