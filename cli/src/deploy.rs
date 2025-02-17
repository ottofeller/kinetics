use crate::client::Client;
use crate::crat;
use crate::function::Function;
use crate::functions;
use crate::secret::Secret;
use backend::crat::Crate;
use backend::deploy::{self, BodyCrate};
use backend::function::Function as BackendFunction;
use eyre::{Ok, WrapErr};
use reqwest::StatusCode;
use std::collections::HashMap;

/// Bundle assets and upload to S3, assuming all functions are built
fn bundle(functions: &Vec<Function>) -> eyre::Result<()> {
    for function in functions {
        function.bundle()?;
    }

    Ok(())
}

/// Call the /upload endpoint to get the presigned URL and upload the file
async fn upload(
    client: &Client,
    functions: &mut Vec<Function>,
    is_directly: &bool,
) -> eyre::Result<()> {
    for function in functions {
        #[derive(serde::Deserialize, Debug)]
        struct PresignedUrl {
            url: String,
            s3key_encrypted: String,
        }

        let path = function.bundle_path();
        println!("Uploading {path:?}...");

        if *is_directly {
            // Upload the backaend manually if the /upload endpoint gets
            // deleted accidentally
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
                .bucket("kinetics-rust-builds")
                .key(direct_s3key)
                .body(body)
                .send()
                .await
                .wrap_err("Failed to upload file to S3")?;

            continue;
        }

        let presigned = client
            .post("/upload")
            .send()
            .await?
            .json::<PresignedUrl>()
            .await?;

        let public_client = reqwest::Client::new();

        public_client
            .put(&presigned.url)
            .body(tokio::fs::read(&path).await?)
            .send()
            .await?
            .error_for_status()?;

        function.set_s3key_encrypted(presigned.s3key_encrypted);
    }

    Ok(())
}

/// Build and deploy all assets using CFN template
pub async fn deploy(is_directly: &bool) -> eyre::Result<()> {
    let crat = crat().unwrap();
    let mut functions = functions().wrap_err("Failed to bundle assets")?;
    let client = crate::client::Client::new(is_directly).wrap_err("Failed to create client")?;
    println!("Deploying \"{}\"...", crat.name);
    bundle(&functions)?;
    upload(&client, &mut functions, is_directly).await?;
    let mut secrets = HashMap::new();

    Secret::from_dotenv()?.iter().for_each(|s| {
        secrets.insert(s.name.clone(), s.value());
    });

    // Provision the template directly if the flag is set
    if *is_directly {
        let crat = Crate::new(crat.toml_string.clone()).wrap_err("Invalid crate toml")?;

        let secrets = secrets
            .iter()
            .map(|(k, v)| backend::secret::Secret::new(k, v, &crat, "nide"))
            .collect::<Vec<backend::secret::Secret>>();

        let template = backend::template::Template::new(
            &crat,
            functions
                .iter()
                .map(|f| {
                    BackendFunction::new(
                        &f.toml_string().unwrap(),
                        &crat,
                        &f.s3key_encrypted.to_owned().unwrap(),
                        "",
                        false,
                    )
                    .unwrap()
                })
                .collect::<Vec<BackendFunction>>(),
            secrets.clone(),
            "kinetics-rust-builds",
            "artemATottofellerDOTcom",
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

    println!("Done!");
    Ok(())
}
