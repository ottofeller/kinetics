use backend::deploy::{self, BodyCrate};
use crate::function::Function;
use crate::crat;
use crate::functions;
use crate::api_url;
use eyre::{Ok, WrapErr};
use crate::secret::Secret;

/// Bundle assets and upload to S3, assuming all functions are built
fn bundle(functions: &Vec<Function>) -> eyre::Result<()> {
    for function in functions {
        function.bundle()?;
    }

    Ok(())
}

/// All bundled assets to S3
async fn upload(functions: &Vec<Function>) -> eyre::Result<()> {
    for function in functions {
        #[derive(serde::Deserialize)]
        struct PresignedUrl {
            url: String,
        }

        let path = function.bundle_path();
        let key = path.file_name().unwrap().to_str().unwrap();
        println!("Uploading {path:?}...");
        let client = reqwest::Client::new();

        let presigned = client
            .post(api_url("/upload"))
            .json(&serde_json::json!({ "key": key }))
            .send()
            .await?
            .json::<PresignedUrl>()
            .await?;

        client
            .put(&presigned.url)
            .body(tokio::fs::read(&path).await?)
            .send()
            .await?
            .error_for_status()?;
    }

    Ok(())
}

/// Build and deploy all assets using CFN template
pub async fn deploy() -> eyre::Result<()> {
    let crat = crat().unwrap();
    let functions = functions().wrap_err("Failed to bundle assets")?;
    let client = reqwest::Client::new();
    println!("Deploying \"{}\"...", crat.name);
    bundle(&functions)?;
    upload(&functions).await?;

    client
        .post(api_url("/deploy"))
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
            secrets: vec![],
        }))
        .send()
        .await
        .wrap_err("Deployment request failed")?;

    let secrets = Secret::from_dotenv(&crat.name)?;

    for secret in secrets.iter() {
        secret.sync().await?;
    }

    println!("Done!");
    Ok(())
}
