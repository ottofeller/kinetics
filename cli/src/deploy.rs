use crate::client::Client;
use crate::config::config as build_config;
use crate::crat::Crate;
use crate::function::Function;
use crate::secret::Secret;
use backend::deploy::{self, BodyCrate};
use backend::template::Crate as BackendCrate;
use backend::template::Function as BackendFunction;
use eyre::{Ok, WrapErr};
use reqwest::StatusCode;
use std::collections::HashMap;

/// Call the /upload endpoint to get the presigned URL and upload the file
pub async fn upload(
    client: &Client,
    function: &mut Function,
    is_directly: &bool,
) -> eyre::Result<()> {
    #[derive(serde::Deserialize, Debug)]
    struct PreSignedUrl {
        url: String,
        s3key_encrypted: String,
    }

    let path = function.bundle_path();

    if *is_directly {
        // Upload the backend manually if the /upload endpoint gets deleted accidentally
        use aws_config::BehaviorVersion;
        use aws_sdk_s3::Client;
        let body = function.zip_stream().await?;
        let config = aws_config::defaults(BehaviorVersion::v2024_03_28())
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

        return Ok(());
    }

    let presigned = client
        .post("/upload")
        .send()
        .await?
        .json::<PreSignedUrl>()
        .await?;

    let public_client = reqwest::Client::new();

    public_client
        .put(&presigned.url)
        .body(tokio::fs::read(&path).await?)
        .send()
        .await?
        .error_for_status()?;

    function.set_s3key_encrypted(presigned.s3key_encrypted);

    Ok(())
}

/// Deploy all assets using CFN template
pub async fn deploy(crat: &Crate, functions: &[Function], is_directly: &bool) -> eyre::Result<()> {
    let client = Client::new(is_directly).wrap_err("Failed to create client")?;
    let mut secrets = HashMap::new();

    Secret::from_dotenv()?.iter().for_each(|s| {
        secrets.insert(s.name.clone(), s.value());
    });

    // Provision the template directly if the flag is set
    if *is_directly {
        let build_config = build_config();
        let crat = BackendCrate::new(crat.toml_string.clone()).wrap_err("Invalid crate toml")?;

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

    let result = client
        .post("/deploy")
        .json(&serde_json::json!(deploy::JsonBody {
            crat: BodyCrate {
                toml: crat.toml_string.clone(),
            },
            functions: functions
                .iter()
                .map(|f| {
                    deploy::BodyFunction {
                        name: f.name().unwrap().to_string(),
                        s3key_encrypted: f.s3key_encrypted().unwrap(),
                        toml: f.toml_string().unwrap(),
                    }
                })
                .collect(),
            secrets: secrets.clone(),
        }))
        .send()
        .await
        .wrap_err("Deployment request failed")?;

    if result.status() != StatusCode::OK {
        return Err(eyre::eyre!("Deployment failed: {:?}", result));
    }

    Ok(())
}
