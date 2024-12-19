use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use clap::{Parser, Subcommand};
use eyre::{eyre, ContextCompat, Ok, WrapErr};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

fn skypath() -> eyre::Result<PathBuf> {
    Ok(Path::new(&std::env::var("HOME").wrap_err("Can not read HOME env var")?).join(".sky"))
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Build,
    Deploy,
}

struct Crate {
    name: String,
}

/// Return crate info from Cargo.toml
fn project() -> eyre::Result<Crate> {
    let cargo_toml: toml::Value =
        cargotoml(&std::env::current_dir().wrap_err("Failed to get current dir")?)?;

    Ok(Crate {
        name: cargo_toml
            .get("package")
            .and_then(|pkg| pkg.get("name"))
            .and_then(|name| name.as_str())
            .wrap_err("Failed to get crate name from Cargo.toml")?
            .to_string(),
    })
}

/// Return the list of dirs with functions to deploy
fn functions() -> eyre::Result<Vec<PathBuf>> {
    let mut result = vec![];
    let project = project()?;

    for entry in std::fs::read_dir(
        &skypath()
            .wrap_err("Failed to resolve sky path")?
            .join(project.name),
    )
    .wrap_err("Failed to read dir")?
    {
        let path = entry.wrap_err("Failed to get dir entry")?.path();

        if path.is_dir() {
            result.push(path);
        }
    }

    Ok(result)
}

/// CFN template for a function — a function itself and its role
fn function2template(name: &str) -> String {
    format!(
        "
        LambdaFunction{name}:
          Type: 'AWS::Lambda::Function'
          Properties:
                FunctionName: {name}
                Handler: bootstrap
                Runtime: provided.al2023
                Role:
                  Fn::GetAtt:
                    - LambdaFunctionRole{name}
                    - Arn
                MemorySize: 1024
                Code:
                  S3Bucket: my-lambda-function-code-test
                  S3Key: bootstrap.zip
        LambdaFunctionRole{name}:
          Type: AWS::IAM::Role
          Properties:
            AssumeRolePolicyDocument:
              Version: '2012-10-17'
              Statement:
              - Effect: Allow
                Principal:
                  Service:
                  - lambda.amazonaws.com
                Action:
                - sts:AssumeRole
            Path: \"/\"
            Policies:
            - PolicyName: AppendToLogsPolicy
              PolicyDocument:
                Version: '2012-10-17'
                Statement:
                - Effect: Allow
                  Action:
                  - logs:CreateLogGroup
                  - logs:CreateLogStream
                  - logs:PutLogEvents
                  Resource: \"*\"
        LambdaFuncrionUrl{name}:
            Type : AWS::Lambda::Url
            Properties:
                AuthType: NONE
                TargetFunctionArn: !Ref LambdaFunction{name}
        LambdaFuncrionUrlPermission{name}:
            Type: AWS::Lambda::Permission
            Properties:
                Action: lambda:InvokeFunctionUrl
                FunctionUrlAuthType: 'NONE'
                FunctionName: !Ref LambdaFunction{name}
                Principal: '*'
        "
    )
}

/// Build all assets and CFN templates
fn build() -> eyre::Result<()> {
    let project = project()?;
    println!("Building \"{}\"...", project.name);

    for path in functions()? {
        let status = std::process::Command::new("cargo")
            .arg("lambda")
            .arg("build")
            .current_dir(&path)
            .output()
            .expect("Failed to execute process")
            .status;

        if !status.success() {
            panic!("Build failed: {:?}", path);
        }
    }

    println!("Done!");
    Ok(())
}

/// Read Cargo.toml in a given dir
fn cargotoml(path: &Path) -> eyre::Result<toml::Value> {
    std::fs::read_to_string(path.join("Cargo.toml"))
        .wrap_err("Failed to read Cargo.toml: {cargo_toml_path:?}")?
        .parse::<toml::Value>()
        .wrap_err("Failed to parse Cargo.toml")
}

/// Generate CFN template for all functions
fn template(functions: Vec<PathBuf>) -> eyre::Result<String> {
    let mut template = "Resources:".to_string();

    for path in functions {
        let cargo_toml: toml::Value = cargotoml(&path)?;

        template.push_str(&function2template(
            cargo_toml
                .get("metadata")
                .wrap_err("No [metadata]")?
                .get("sky")
                .wrap_err("No [sky]")?
                .get("function")
                .wrap_err("No [function]")?
                .as_array()
                .wrap_err("msg")?[0]
                .get("name")
                .wrap_err("No [name]")?
                .as_str()
                .wrap_err("Not a string")?,
        ));

        template.push_str("\n");
    }

    Ok(template)
}

/// Bundle assets and upload to S3, assuming all functions are built
fn bundle(functions: &Vec<PathBuf>) -> eyre::Result<()> {
    for path in functions {
        println!("Building {path:?} with cargo-lambda...");

        let status = std::process::Command::new("cargo")
            .arg("lambda")
            .arg("build")
            .arg("--release")
            .current_dir(&path)
            .output()
            .wrap_err("Failed to execute the process")?
            .status;

        if !status.success() {
            Err(eyre!("Build failed: {path:?} {:?}", status.code()))?;
        }

        println!("Bundling {path:?}...");
        let file = std::fs::File::create(&path.join("bootstrap.zip"))?;
        let mut zip = zip::ZipWriter::new(file);

        let mut f = std::fs::File::open(
            path.join("target")
                .join("lambda")
                .join("bootstrap")
                .join("bootstrap")
                .to_str()
                .ok_or(eyre!("Failed to construct asset path"))?,
        )?;

        zip.start_file("bootstrap", SimpleFileOptions::default())?;
        std::io::copy(&mut f, &mut zip)?;
        zip.finish()?;
    }

    Ok(())
}

/// All bundled assets to S3
async fn upload(functions: &Vec<PathBuf>) -> eyre::Result<()> {
    for path in functions {
        let bucket_name = "my-lambda-function-code-test";
        let key = "bootstrap.zip";
        let body = aws_sdk_s3::primitives::ByteStream::from_path(path.join("bootstrap.zip")).await;

        let config = aws_config::defaults(BehaviorVersion::v2024_03_28())
            .load()
            .await;

        let client = Client::new(&config);

        client
            .put_object()
            .bucket(bucket_name)
            .key(key)
            .body(body.unwrap())
            .send()
            .await
            .wrap_err("Failed to upload file to S3")?;
    }

    Ok(())
}

/// Provision cloud resources using CFN template
async fn provision(template: &str) -> eyre::Result<()> {
    let config = aws_config::defaults(BehaviorVersion::v2024_03_28())
        .load()
        .await;

    let client = aws_sdk_cloudformation::Client::new(&config);

    client
        .create_stack()
        .capabilities(aws_sdk_cloudformation::types::Capability::CapabilityIam)
        .stack_name("sky-example")
        .template_body(template)
        .send()
        .await
        .wrap_err("Failed to create stack")?;

    Ok(())
}

/// Build and deploy all assets using CFN template
async fn deploy() -> eyre::Result<()> {
    let crat = project().unwrap();
    let functions = functions().wrap_err("Failed to bundle assets")?;
    println!("Deploying \"{}\"...", crat.name);
    bundle(&functions)?;
    upload(&functions).await?;
    let template = template(functions)?;
    provision(&template).await?;
    println!("{template}");
    println!("Done!");
    Ok(())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Build) => {
            if let Err(error) = build() {
                println!("{error}");
                return;
            }
        }
        Some(Commands::Deploy) => {
            if let Err(error) = build() {
                println!("{error:?}");
                return;
            }

            if let Err(error) = deploy().await {
                println!("{error:?}");
                return;
            }
        }
        None => {}
    }
}
