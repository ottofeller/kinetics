use crate::client::Client;
use backend::deploy::{self, BodyCrate};
use crate::crat;
use crate::function::Function;
use crate::functions;
use crate::secret::Secret;
use eyre::{Ok, WrapErr};
use std::collections::HashMap;

/// Bundle assets and upload to S3, assuming all functions are built
fn bundle(functions: &Vec<Function>) -> eyre::Result<()> {
    for function in functions {
        function.bundle()?;
    }

    Ok(())
}

/// Call the /upload endpoint to get the presigned URL and upload the file
async fn upload(client: &Client, functions: &Vec<Function>) -> eyre::Result<()> {
    for function in functions {
        #[derive(serde::Deserialize)]
        struct PresignedUrl {
            url: String,
        }

        let path = function.bundle_path();
        let key = path.file_name().unwrap().to_str().unwrap();
        println!("Uploading {path:?}...");

        let presigned = client
            .post("/upload")
            .json(&serde_json::json!({ "key": key }))
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

        // Upload the backaend manually if the /upload endpoint gets
        // deleted accidentally
        // use aws_config::BehaviorVersion;
        // use aws_sdk_s3::Client;
        // let body = function.zip_stream().await?;
        // let config = aws_config::defaults(BehaviorVersion::v2024_03_28())
        //     .load()
        //     .await;

        // let client = Client::new(&config);

        // client
        //     .put_object()
        //     .bucket("kinetics-rust-builds")
        //     .key(key)
        //     .body(body)
        //     .send()
        //     .await
        //     .wrap_err("Failed to upload file to S3")?;
    }

    Ok(())
}

/// Build and deploy all assets using CFN template
pub async fn deploy() -> eyre::Result<()> {
    let crat = crat().unwrap();
    let functions = functions().wrap_err("Failed to bundle assets")?;
    let client = crate::client::Client::new().wrap_err("Failed to create client")?;
    println!("Deploying \"{}\"...", crat.name);
    bundle(&functions)?;
    upload(&client, &functions).await?;
    let mut secrets = HashMap::new();

    Secret::from_dotenv()?.iter().for_each(|s| {
        secrets.insert(s.name.clone(), s.value());
    });

    client.post("/deploy")
        .json(&serde_json::json!(deploy::JsonBody {
            crat: BodyCrate {
                toml: crat.toml_string.clone(),
            },
            functions: functions
                .iter()
                .map(|f| {
                    deploy::BodyFunction {
                        name: f.name().unwrap().to_string(),
                        s3key: f.bundle_name(),
                        toml: f.toml_string().unwrap(),
                    }
                })
                .collect(),
            secrets: secrets.clone(),
        }))
        .send()
        .await
        .wrap_err("Deployment request failed")?;

    println!("Done!");
    Ok(())
}
